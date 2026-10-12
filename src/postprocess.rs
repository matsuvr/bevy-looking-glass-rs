//! Bevy 0.20's DoF extractor accepts only Projection::Perspective. A settings-only
//! child lets that extractor compute its own uniform, then transfers the uniform
//! to the real off-axis camera. Rendering/culling always keep the optical lens.
use crate::{LookingGlassSystems, LookingGlassViewCamera, optics::OffAxisProjection};
use bevy::{
    post_process::dof::{DepthOfField, DepthOfFieldUniform},
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        sync_world::MainEntity,
    },
};

#[derive(Component, Clone, ExtractComponent)]
#[extract_app(RenderApp)]
struct DofSettings {
    camera: Entity,
}

pub(crate) fn install(app: &mut App) {
    app.add_plugins(ExtractComponentPlugin::<DofSettings>::default())
        .add_systems(PostUpdate, sync.after(LookingGlassSystems::SyncViews));
    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
        render_app.add_systems(
            Render,
            transfer
                .after(RenderSystems::ExtractCommands)
                .before(RenderSystems::PrepareViews),
        );
    }
}

fn sync(
    mut commands: Commands,
    cameras: Query<(Entity, &Projection, &DepthOfField), With<LookingGlassViewCamera>>,
    mut settings: Query<
        (Entity, &DofSettings, &mut Projection, &mut DepthOfField),
        Without<LookingGlassViewCamera>,
    >,
) {
    for (entity, settings, mut projection, mut dof) in &mut settings {
        let Ok((_, lens, current)) = cameras.get(settings.camera) else {
            commands.entity(entity).despawn();
            continue;
        };
        if let Projection::Custom(lens) = lens
            && let Some(lens) = lens.get::<OffAxisProjection>()
        {
            *projection = Projection::Perspective(lens.perspective.clone());
            *dof = *current;
        }
    }
    for (camera, projection, dof) in &cameras {
        if settings
            .iter()
            .any(|(_, settings, _, _)| settings.camera == camera)
        {
            continue;
        }
        if let Projection::Custom(lens) = projection
            && let Some(lens) = lens.get::<OffAxisProjection>()
        {
            commands.spawn((
                DofSettings { camera },
                Projection::Perspective(lens.perspective.clone()),
                *dof,
                ChildOf(camera),
            ));
        }
    }
}

fn transfer(
    mut commands: Commands,
    settings: Query<(&DofSettings, &DepthOfField, &DepthOfFieldUniform)>,
    cameras: Query<(Entity, &MainEntity), With<crate::view_array::ViewCamera>>,
) {
    for (settings, dof, uniform) in &settings {
        if let Some((entity, _)) = cameras
            .iter()
            .find(|(_, main)| main.id() == settings.camera)
        {
            commands.entity(entity).insert((*dof, *uniform));
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use bevy::camera::CameraProjection;

    #[test]
    fn dof_adapter_preserves_optical_lens_and_follows_setting_removal() {
        let lens = OffAxisProjection {
            perspective: PerspectiveProjection {
                fov: 0.7,
                ..default()
            },
            offset_over_focus: 0.3,
        };
        let clip = lens.get_clip_from_view();
        let mut app = App::new();
        app.add_systems(Update, sync);
        let owner = app.world_mut().spawn_empty().id();
        let camera = app
            .world_mut()
            .spawn((
                LookingGlassViewCamera {
                    output: owner,
                    index: 4,
                },
                Projection::custom(lens),
                DepthOfField {
                    focal_distance: 5.0,
                    ..default()
                },
            ))
            .id();
        app.update();
        let world = app.world_mut();
        let (proxy, projection, dof) = world
            .query_filtered::<(Entity, &Projection, &DepthOfField), With<DofSettings>>()
            .single(world)
            .unwrap();
        assert!(matches!(projection, Projection::Perspective(p) if p.fov == 0.7));
        assert_eq!(dof.focal_distance, 5.0);
        assert_eq!(
            world
                .get::<Projection>(camera)
                .unwrap()
                .get_clip_from_view(),
            clip
        );
        world.entity_mut(camera).remove::<DepthOfField>();
        app.update();
        assert!(app.world().get_entity(proxy).is_err());
        assert!(app.world().get_entity(camera).is_ok());
    }
}
