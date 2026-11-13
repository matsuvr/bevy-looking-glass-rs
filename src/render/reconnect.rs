//! Rediscover only after losing the OS monitor used by a Bridge-placed output.
use super::{OutputWindow, SelectedMonitor, clear_output, monitor_info, start_output};
use crate::{ActiveOutput, LookingGlassConfig, LookingGlassStatus, bridge, plan_output_window};
use bevy::{
    ecs::message::MessageCursor,
    prelude::*,
    window::{ClosingWindow, Monitor, WindowCloseRequested, WindowDestroyed},
};
use std::{
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const FIRST_RETRY: Duration = Duration::from_secs(1);
const MAX_RETRY: Duration = Duration::from_secs(10);

#[derive(Resource)]
pub(super) struct Reconnect {
    // A window entity identifies one specific start, even for the same serial.
    previous_window: Entity,
    config: LookingGlassConfig,
    next_attempt: Instant,
    delay: Duration,
}

// Keep the single outstanding worker until it finishes, even after stop/replace.
// It owns no window, GPU handles or configuration.
#[derive(Resource)]
struct Probe {
    previous_window: Entity,
    worker: JoinHandle<Result<Vec<bridge::Display>, bridge::Error>>,
}

pub(super) fn check_connection(
    world: &mut World,
    mut destroyed: Local<MessageCursor<WindowDestroyed>>,
    mut close_requested: Local<MessageCursor<WindowCloseRequested>>,
) {
    if let Some(target) = world.get_resource::<SelectedMonitor>() {
        let window = target.window;
        let plan = target.plan;
        let was_closing = target.closing;
        let connected = world.get::<Monitor>(plan.monitor).is_some_and(|monitor| {
            monitor.physical_position == plan.position && monitor.physical_size() == plan.size
        });
        let closed_by_os = destroyed
            .read(world.resource())
            .any(|event| event.window == window);
        let user_closed = close_requested
            .read(world.resource())
            .any(|event| event.window == window);
        let window_exists = world.get::<OutputWindow>(window).is_some();
        let marked_closing = world.get::<ClosingWindow>(window).is_some();
        let closing = was_closing || user_closed || marked_closing;
        if !closing
            && (!connected || closed_by_os)
            && let Some(ActiveOutput(config)) = world.remove_resource::<ActiveOutput>()
        {
            clear_output(world, window_exists.then_some(window));
            world.resource_mut::<LookingGlassStatus>().error =
                Some("Looking Glass output disconnected; waiting to reconnect".into());
            world.insert_resource(Reconnect {
                previous_window: window,
                config,
                next_attempt: Instant::now() + FIRST_RETRY,
                delay: FIRST_RETRY,
            });
        } else if !window_exists {
            clear_output(world, None);
        } else if marked_closing != was_closing {
            // Bevy despawns a window on the frame after it marks it closing.
            world.resource_mut::<SelectedMonitor>().closing = marked_closing;
        }
    }

    if let Some(probe) = world.get_resource::<Probe>() {
        if !probe.worker.is_finished() {
            return;
        }
        if let Some(probe) = world.remove_resource::<Probe>() {
            let result = probe.worker.join();
            if world
                .get_resource::<Reconnect>()
                .is_some_and(|retry| retry.previous_window == probe.previous_window)
            {
                let result = result
                    .map_err(|_| "Bridge discovery worker failed".to_string())
                    .and_then(|result| result.map_err(|err| err.to_string()))
                    .and_then(|displays| resume(world, displays));
                if let Err(error) = result {
                    retry_later(world, error);
                }
            }
        }
    }

    let Some(retry) = world.get_resource::<Reconnect>() else {
        return;
    };
    if Instant::now() < retry.next_attempt {
        return;
    }
    let previous_window = retry.previous_window;
    match thread::Builder::new()
        .name("looking-glass-reconnect".into())
        .spawn(bridge::discover)
    {
        Ok(worker) => world.insert_resource(Probe {
            previous_window,
            worker,
        }),
        Err(error) => retry_later(world, error.to_string()),
    }
}

fn retry_later(world: &mut World, error: String) {
    let mut retry = world.resource_mut::<Reconnect>();
    retry.next_attempt = Instant::now() + retry.delay;
    retry.delay = (retry.delay * 2).min(MAX_RETRY);
    world.resource_mut::<LookingGlassStatus>().error =
        Some(format!("Waiting to reconnect Looking Glass: {error}"));
}

fn resume(world: &mut World, displays: Vec<bridge::Display>) -> Result<(), String> {
    let retry = world.resource::<Reconnect>();
    let original = retry
        .config
        .display
        .as_ref()
        .ok_or("missing selected display")?;
    let mut matches = displays
        .into_iter()
        .filter(|display| display.serial == original.serial);
    let display = matches
        .next()
        .ok_or_else(|| format!("{} is absent", original.serial))?;
    if matches.next().is_some() {
        return Err(format!("ambiguous Bridge serial {}", original.serial));
    }
    super::validate_source(&retry.config, world).map_err(|err| err.to_string())?;
    let screen = retry.config.output.screen_size();
    let plan = plan_output_window(&display, &monitor_info(world)).map_err(|err| err.to_string())?;
    if plan.size != screen {
        return Err("reconnected display has a different native resolution".into());
    }

    // Layout, source and optics belong to the host. Only placement comes from
    // this new discovery. All structural/GPU settings passed start validation.
    let mut retry = world
        .remove_resource::<Reconnect>()
        .ok_or("output was stopped")?;
    retry.config.display = Some(display);
    world.resource_mut::<LookingGlassStatus>().error = None;
    start_output(world, retry.config);
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::super::{PendingFullscreen, tests::bridge_app};
    use super::*;
    use crate::{LookingGlassControl, LookingGlassViewCamera};
    use bevy::window::{MonitorSelection, PrimaryWindow, WindowCreated, WindowMode};
    use std::sync::mpsc;

    fn output(app: &App) -> Entity {
        app.world().resource::<LookingGlassStatus>().window.unwrap()
    }
    fn display(app: &App) -> bridge::Display {
        app.world()
            .resource::<ActiveOutput>()
            .0
            .display
            .clone()
            .unwrap()
    }

    // Feed the worker completion through the normal update path without HTTP or sleeping.
    fn complete(
        app: &mut App,
        previous_window: Entity,
        result: Result<Vec<bridge::Display>, bridge::Error>,
    ) {
        let worker = thread::spawn(move || result);
        while !worker.is_finished() {
            thread::yield_now();
        }
        app.world_mut().insert_resource(Probe {
            previous_window,
            worker,
        });
        app.update();
    }

    #[test]
    fn disconnected_output_waits_for_its_serial_and_current_monitor_then_recreates_once() {
        let (mut app, monitor) = bridge_app();
        let window = output(&app);
        let main = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        let mut hardware = display(&app);
        let mut os_monitor = app.world().get::<Monitor>(monitor).unwrap().clone();
        app.world_mut().write_message(WindowCreated { window });
        app.update();
        assert!(app.world().get::<PendingFullscreen>(window).is_none());
        app.world_mut().despawn(monitor);
        app.update();
        assert!(app.world().get_entity(window).is_err());
        let world = app.world_mut();
        assert_eq!(
            world.query::<&LookingGlassViewCamera>().iter(world).count(),
            0
        );
        assert!(!world.contains_resource::<Probe>());
        assert!(world.contains_resource::<Reconnect>());
        complete(&mut app, window, Err(bridge::Error::Unavailable));
        assert!(
            app.world()
                .resource::<LookingGlassStatus>()
                .error
                .as_deref()
                .unwrap()
                .contains("unavailable")
        );
        let mut other = hardware.clone();
        other.serial = "OTHER".into();
        complete(&mut app, window, Ok(vec![other.clone()]));
        assert!(
            app.world()
                .resource::<LookingGlassStatus>()
                .window
                .is_none()
        );
        // The OS and Bridge can recover in different frames, with a new position and index.
        hardware.index += 1;
        hardware.window_position.x -= 120;
        os_monitor.physical_position = hardware.window_position;
        complete(&mut app, window, Ok(vec![hardware.clone()]));
        assert!(
            app.world()
                .resource::<LookingGlassStatus>()
                .window
                .is_none()
        );
        let new_monitor = app.world_mut().spawn(os_monitor).id();
        complete(&mut app, window, Ok(vec![other, hardware]));
        let restored = output(&app);
        assert_ne!(restored, window);
        app.world_mut()
            .write_message(WindowCreated { window: restored });
        app.update();
        assert_eq!(
            app.world().get::<Window>(restored).unwrap().mode,
            WindowMode::BorderlessFullscreen(MonitorSelection::Entity(new_monitor))
        );
        assert_eq!(
            app.world().get::<Window>(main).unwrap().mode,
            WindowMode::Windowed
        );
        assert!(app.world().resource::<LookingGlassStatus>().error.is_none());
        assert!(!app.world().contains_resource::<Reconnect>());
        assert!(!app.world().contains_resource::<Probe>());
        assert_eq!(output(&app), restored);
    }

    #[test]
    fn stop_during_pending_placement_cancels_even_an_in_flight_reply() {
        let (mut app, monitor) = bridge_app();
        let window = output(&app);
        let hardware = display(&app);
        app.world_mut().despawn(monitor);
        app.world_mut().write_message(WindowCreated { window });
        app.update();
        assert!(
            app.world()
                .resource::<LookingGlassStatus>()
                .error
                .as_deref()
                .unwrap()
                .contains("disconnected")
        );
        let (send, recv) = mpsc::channel();
        let worker = thread::spawn(move || recv.recv().unwrap());
        app.world_mut().insert_resource(Probe {
            previous_window: window,
            worker,
        });
        app.world_mut().resource_mut::<LookingGlassControl>().stop();
        app.update();
        assert!(!app.world().contains_resource::<Reconnect>());
        send.send(Ok(vec![hardware])).unwrap();
        while !app.world().resource::<Probe>().worker.is_finished() {
            thread::yield_now();
        }
        app.update();
        assert!(!app.world().contains_resource::<Probe>());
        assert!(
            app.world()
                .resource::<LookingGlassStatus>()
                .window
                .is_none()
        );
        assert!(app.world().resource::<LookingGlassStatus>().error.is_none());
    }

    #[test]
    fn simultaneous_monitor_and_window_removal_keeps_the_selected_display() {
        let (mut app, monitor) = bridge_app();
        let window = output(&app);
        app.world_mut().despawn(window);
        app.world_mut().despawn(monitor);
        app.update();
        assert!(app.world().contains_resource::<Reconnect>());
        assert!(
            app.world()
                .resource::<LookingGlassStatus>()
                .window
                .is_none()
        );
    }

    #[test]
    fn unexpected_native_destruction_recovers_but_an_explicit_close_does_not() {
        for user_closed in [true, false] {
            let (mut app, _) = bridge_app();
            let window = output(&app);
            if user_closed {
                app.world_mut()
                    .write_message(WindowCloseRequested { window });
                app.world_mut().entity_mut(window).insert(ClosingWindow);
            }
            app.world_mut().write_message(WindowDestroyed { window });
            app.update();
            assert_eq!(app.world().contains_resource::<Reconnect>(), !user_closed);
        }
    }
}
