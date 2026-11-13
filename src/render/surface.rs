//! Recover a stalled presentation surface without rebuilding the scene or contacting Bridge.
use bevy::{
    camera::{CameraOutputMode, NormalizedRenderTarget},
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        camera::SortedCameras,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        sync_world::MainEntity,
        view::{ExtractedWindow, SurfaceData, prepare_windows},
    },
    window::WindowOccluded,
};
use std::time::{Duration, Instant};

/// Present on Bridge-placed windows only.
#[derive(Component, Clone, Copy, PartialEq, Eq, ExtractComponent)]
#[extract_app(RenderApp)]
pub(super) struct WatchedSurface {
    pub visible: bool,
}

pub(super) fn install(app: &mut App) {
    app.add_plugins(ExtractComponentPlugin::<WatchedSurface>::default())
        .add_systems(Update, track_visibility);
    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app.add_systems(
            Render,
            recover_surface
                .in_set(RenderSystems::PrepareViews)
                .after(prepare_windows),
        );
    }
}

fn track_visibility(
    mut events: MessageReader<WindowOccluded>,
    mut windows: Query<(Entity, &Window, &mut WatchedSurface)>,
    mut occluded: Local<Option<Entity>>,
) {
    for event in events.read() {
        if windows.contains(event.window) {
            *occluded = event.occluded.then_some(event.window);
        }
    }
    for (entity, window, mut watched) in &mut windows {
        watched.set_if_neq(WatchedSurface {
            visible: window.visible
                && *occluded != Some(entity)
                && window.physical_width() > 0
                && window.physical_height() > 0,
        });
    }
}

#[derive(Default)]
struct AcquisitionFailures {
    window: Option<Entity>,
    retry_at: Option<Instant>,
    attempts: u32,
}

impl AcquisitionFailures {
    // Require sustained AND repeated misses. An isolated timeout, a long frame
    // or the first frame after waking from sleep is insufficient evidence.
    fn should_recreate(&mut self, window: Entity, now: Instant) -> bool {
        if self.window != Some(window) {
            *self = Self {
                window: Some(window),
                retry_at: Some(now + Duration::from_secs(2)),
                attempts: 0,
            };
        }
        self.attempts = self.attempts.saturating_add(1);
        if self.attempts >= 3 && self.retry_at.is_some_and(|at| now >= at) {
            self.attempts = 0;
            self.retry_at = Some(now + Duration::from_secs(10));
            return true;
        }
        false
    }
}

fn recover_surface(
    mut commands: Commands,
    windows: Query<(Entity, &MainEntity, &ExtractedWindow, &WatchedSurface), With<SurfaceData>>,
    cameras: Res<SortedCameras>,
    mut failures: Local<AcquisitionFailures>,
) {
    let Ok((entity, main, window, watched)) = windows.single() else {
        return;
    };
    if !watched.visible || window.swap_chain_texture.is_some()
        || !cameras.0.iter().any(|camera| {
            matches!(camera.target, Some(NormalizedRenderTarget::Window(target)) if target.entity() == main.id())
                && matches!(camera.output_mode, CameraOutputMode::Write { .. })
        })
    {
        *failures = AcquisitionFailures::default();
        return;
    }
    if failures.should_recreate(entity, Instant::now()) {
        // Bevy's prepare_windows can log a lost surface and leave it installed.
        // create_surfaces builds a fresh one from this window's handle next frame.
        commands.entity(entity).remove::<SurfaceData>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sustained_acquisition_failures_retry_with_a_cooldown_scoped_to_one_window() {
        let mut world = World::new();
        let a = world.spawn_empty().id();
        let b = world.spawn_empty().id();
        let now = Instant::now();
        let mut state = AcquisitionFailures::default();
        let at = |seconds| now + Duration::from_secs(seconds);
        assert!(!state.should_recreate(a, at(0)));
        assert!(!state.should_recreate(a, at(1)));
        assert!(state.should_recreate(a, at(2)));
        assert!(!state.should_recreate(a, at(3)));
        assert!(!state.should_recreate(a, at(4)));
        assert!(!state.should_recreate(a, at(11)));
        assert!(state.should_recreate(a, at(12)));
        assert!(!state.should_recreate(b, at(50)));
        assert!(!state.should_recreate(b, at(100)));
    }
}
