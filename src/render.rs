use bevy_derive::{Deref, DerefMut};
use bevy_ecs::prelude::Query;
use bevy_ecs::{
    system::{Commands, Res, Resource},
    world::World,
};
use bevy_render::renderer::RenderDevice;
use bevy_render::{
    render_graph::{Node, NodeRunError, RenderGraphContext},
    renderer::RenderContext,
    view::ExtractedWindows,
    Extract,
};
use bevy_window::Window;
use iced_native::Size;
use iced_wgpu::{wgpu::util::StagingBelt, Viewport};
use std::sync::Mutex;

use crate::{DidDraw, IcedProps, IcedResource, IcedSettings};

pub const ICED_PASS: &'static str = "bevy_iced_pass";

#[derive(Resource, Deref, DerefMut, Clone)]
pub struct ViewportResource(pub Viewport);

pub(crate) fn update_viewport(
    windows: Query<&Window>,
    iced_settings: Res<IcedSettings>,
    mut commands: Commands,
) {
    let window = windows.single();
    let scale_factor = iced_settings.scale_factor.unwrap_or(window.scale_factor());
    let viewport = Viewport::with_physical_size(
        Size::new(window.physical_width(), window.physical_height()),
        scale_factor,
    );
    commands.insert_resource(ViewportResource(viewport));
}

// Same as DidDraw, but as a regular bool instead of an atomic.
#[derive(Resource, Deref, DerefMut)]
struct DidDrawBasic(bool);

pub(crate) fn extract_iced_data(
    mut commands: Commands,
    viewport: Extract<Res<ViewportResource>>,
    did_draw: Extract<Res<DidDraw>>,
) {
    commands.insert_resource(viewport.clone());
    commands.insert_resource(DidDrawBasic(
        did_draw.swap(false, std::sync::atomic::Ordering::Relaxed),
    ));
}

pub struct IcedNode {
    staging_belt: Mutex<StagingBelt>,
}

impl IcedNode {
    pub fn new() -> Self {
        Self {
            staging_belt: Mutex::new(StagingBelt::new(5 * 1024)),
        }
    }
}

impl Node for IcedNode {
    fn update(&mut self, _world: &mut World) {
        self.staging_belt.lock().unwrap().recall()
    }

    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let Some(extracted_window) = world
            .get_resource::<ExtractedWindows>()
            .unwrap()
            .windows
            .values()
            .next() else { return Ok(()) };

        let IcedProps {
            renderer, debug, ..
        } = &mut *world.resource::<IcedResource>().lock().unwrap();
        let render_device = world.resource::<RenderDevice>();

        if !world
            .get_resource::<DidDrawBasic>()
            .map(|x| x.0)
            .unwrap_or(false)
        {
            return Ok(());
        }

        let view = extracted_window.swap_chain_texture.as_ref().unwrap();
        let staging_belt = &mut *self.staging_belt.lock().unwrap();

        let viewport = &*world.resource::<ViewportResource>();
        let device = render_device.wgpu_device();

        renderer.with_primitives(|backend, primitives| {
            backend.present(
                device,
                staging_belt,
                render_context.command_encoder(),
                view,
                primitives,
                viewport,
                &debug.overlay(),
            );
        });

        staging_belt.finish();

        Ok(())
    }
}
