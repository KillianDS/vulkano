// Copyright (c) 2016 The vulkano developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

// Welcome to the fullscreen example!
//
// This example is based on the triangle example and will show the basics of
// full screen rendering and how to acquire/release exclusive full screen mode.
// In exclusive full screen mode your application takes ownership of the display,
// allowing for multiple optimizations by bypassing most of the compositor (windowing)
// system. This does mean we need to release and re-acquire exclusive mode when focus is lost.

use vulkano::buffer::{BufferUsage, CpuAccessibleBuffer};
use vulkano::command_buffer::{AutoCommandBufferBuilder, DynamicState};
use vulkano::device::{Device, DeviceExtensions};
use vulkano::framebuffer::{Framebuffer, FramebufferAbstract, Subpass, RenderPassAbstract};
use vulkano::image::SwapchainImage;
use vulkano::instance::{Instance, PhysicalDevice};
use vulkano::pipeline::GraphicsPipeline;
use vulkano::pipeline::viewport::Viewport;
use vulkano::swapchain::{AcquireError, PresentMode, SurfaceTransform, Swapchain, SwapchainCreationError, ColorSpace, FullscreenExclusive};
use vulkano::swapchain;
use vulkano::sync::{GpuFuture, FlushError};
use vulkano::sync;

use vulkano_win::VkSurfaceBuild;
use winit::window::{WindowBuilder, Window, Fullscreen};
use winit::event_loop::{EventLoop, ControlFlow};
use winit::event::{Event, WindowEvent, DeviceEvent, KeyboardInput, VirtualKeyCode, ElementState};


use std::sync::Arc;

fn main() {
    // Push F to toggle, escape to quit
    // The start of this example is exactly the same as `triangle`. You should read the
    // `triangle` example if you haven't done so yet.
    let required_extensions = vulkano_win::required_extensions();
    let instance = Instance::new(None, &required_extensions, None).unwrap();
    let physical = PhysicalDevice::enumerate(&instance).next().unwrap();
    println!("Using device: {} (type: {:?})", physical.name(), physical.ty());
    let event_loop = EventLoop::new();
    //We keep a fullscreen mutable to toggle fullscreen. The initial state can be changed here also, the code can handle both.
    let mut fullscreen = true;
    let surface = if fullscreen {
        let mode = event_loop.available_monitors().next().unwrap().video_modes().max_by_key(|mode|(mode.bit_depth(),mode.size().width*mode.size().height,mode.refresh_rate())).unwrap();    
        WindowBuilder::new().with_fullscreen(Some(Fullscreen::Exclusive(mode))).build_vk_surface(&event_loop, instance.clone()).unwrap()
    } else {
        WindowBuilder::new().build_vk_surface(&event_loop, instance.clone()).unwrap()
    };
    
    let queue_family = physical.queue_families().find(|&q| {
        // We take the first queue that supports drawing to our window.
        q.supports_graphics() && surface.is_supported(q).unwrap_or(false)
    }).unwrap();

    let device_ext = DeviceExtensions { khr_swapchain: true, ext_full_screen_exclusive: true, .. DeviceExtensions::none() };
    let (device, mut queues) = Device::new(physical, physical.supported_features(), &device_ext,
        [(queue_family, 0.5)].iter().cloned()).unwrap();
    let queue = queues.next().unwrap();

    let (mut swapchain, images) = {
        let caps = surface.capabilities(physical).unwrap();
        let usage = caps.supported_usage_flags;
        let alpha = caps.supported_composite_alpha.iter().next().unwrap();
        let format = caps.supported_formats[0].0;
        let dimensions: [u32; 2] = surface.window().inner_size().into();
        // Using min_image_count in fullscreen mode fails on some devices, always try to add one:
        let image_count = caps.max_image_count.unwrap_or(u32::max_value()).min(caps.min_image_count+1);
        // Swapchain is created with an ExplicitAcquire for full screen exclusive mode. This allows us to 
        // give up on fullscreen when we go windowed and vice versa.
        // For simple full-screen only applications, using ExplicitAcquire::Allowed should be good enough.
        Swapchain::new(device.clone(), surface.clone(), image_count, format,
            dimensions, 1, usage, &queue, SurfaceTransform::Identity, alpha,
            PresentMode::Fifo, Some(FullscreenExclusive::ExplicitAcquire), true, ColorSpace::SrgbNonLinear).unwrap()
    };
    if fullscreen {
        swapchain.acquire_full_screen_exclusive_mode().unwrap();
    }
    let vertex_buffer = {
        #[derive(Default, Debug, Clone)]
        struct Vertex { position: [f32; 2] }
        vulkano::impl_vertex!(Vertex, position);

        CpuAccessibleBuffer::from_iter(device.clone(), BufferUsage::all(), false, [
            Vertex { position: [-0.5, -0.25] },
            Vertex { position: [0.0, 0.5] },
            Vertex { position: [0.25, -0.1] }
        ].iter().cloned()).unwrap()
    };
    mod vs {
        vulkano_shaders::shader!{
            ty: "vertex",
            src: "
				#version 450

				layout(location = 0) in vec2 position;

				void main() {
					gl_Position = vec4(position, 0.0, 1.0);
				}
			"
        }
    }

    mod fs {
        vulkano_shaders::shader!{
            ty: "fragment",
            src: "
				#version 450

				layout(location = 0) out vec4 f_color;

				void main() {
					f_color = vec4(1.0, 0.0, 0.0, 1.0);
				}
			"
        }
    }

    let vs = vs::Shader::load(device.clone()).unwrap();
    let fs = fs::Shader::load(device.clone()).unwrap();

    let render_pass = Arc::new(vulkano::single_pass_renderpass!(
        device.clone(),
        attachments: {
            color: {
                load: Clear,
                store: Store,
                format: swapchain.format(),
                // TODO:
                samples: 1,
            }
        },
        pass: {
            color: [color],
            depth_stencil: {}
        }
    ).unwrap());

    let pipeline = Arc::new(GraphicsPipeline::start()
        .vertex_input_single_buffer()
        .vertex_shader(vs.main_entry_point(), ())
        .triangle_list()
        .viewports_dynamic_scissors_irrelevant(1)
        .fragment_shader(fs.main_entry_point(), ())
        .render_pass(Subpass::from(render_pass.clone(), 0).unwrap())
        .build(device.clone())
        .unwrap());
        
    let mut dynamic_state = DynamicState { line_width: None, viewports: None, scissors: None, compare_mask: None, write_mask: None, reference: None };
    let mut framebuffers = window_size_dependent_setup(&images, render_pass.clone(), &mut dynamic_state);
    let mut recreate_swapchain = false;
    let mut focussed = true;
    let mut previous_frame_end = Some(Box::new(sync::now(device.clone())) as Box<dyn GpuFuture>);
    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                *control_flow = ControlFlow::Exit;
            },
            Event::WindowEvent { event: WindowEvent::Focused(focus), .. } => {
                focussed = focus;
                //When losing focus (e.g. alt-tab) we release full_screen exclusive, when regaining focus
                // we always recreate the swapchain to be safe.
                if focus {
                    surface.window().set_minimized(false);
                    recreate_swapchain = true;
                } else {
                    if fullscreen {
                        swapchain.release_full_screen_exclusive_mode().unwrap();
                        surface.window().set_minimized(true);
                    }
                }
            },
            Event::WindowEvent { event: WindowEvent::Resized(_), .. } => {
                recreate_swapchain = true;
            },
            Event::DeviceEvent { event: DeviceEvent::Key(key), .. } => {
                //We support stopping the application by hitting escape
                match key {
                    KeyboardInput{state: ElementState::Released, virtual_keycode: Some(VirtualKeyCode::Escape), .. } => {
                        *control_flow = ControlFlow::Exit;
                    },
                    KeyboardInput{state: ElementState::Released, virtual_keycode: Some(VirtualKeyCode::F), .. } => {
                        recreate_swapchain = true;
                        fullscreen = !fullscreen;
                    },
                    _ => {}
                }
            },
            Event::RedrawEventsCleared => {
                // If we lost focus while fullscreen we can't draw (viewport = 0,0)
                if fullscreen && !focussed { return; }
                previous_frame_end.as_mut().unwrap().cleanup_finished();

                if recreate_swapchain {
                    // We keep a boolean to check if we need to acquire exclusive full screen after toggling to mode.
                    // Not all drivers will succeed toggling acquire on a swapchain linked to a windowed surface, so this 
                    // has to be called after recreating the swapchain.
                    let mut acquire_fullscreen = false;
                    match surface.window().fullscreen() {
                        None => {
                            if fullscreen {
                                let monitor = surface.window().current_monitor();
                                // Find 'best' video mode: first by bit depth, then most pixels, then highest refresh rate
                                let mode = monitor.video_modes().max_by_key(|mode|(mode.bit_depth(),mode.size().width*mode.size().height,mode.refresh_rate())).unwrap();
                                surface.window().set_fullscreen(Some(Fullscreen::Exclusive(mode)));
                                acquire_fullscreen = true;
                            }
                        },
                        Some(_) => {
                            if !fullscreen {
                                surface.window().set_fullscreen(None);
                                //we should already release exclusive mode on the 'fullscreen' swapchain, otherwise the windowed
                                // swapchain might inherit it initially, which is suboptimal and may lead to issues.
                                swapchain.release_full_screen_exclusive_mode().unwrap();
                            }
                        },
                    }
                    let dimensions: [u32; 2] = surface.window().inner_size().into();
                    let (new_swapchain, new_images) = match swapchain.recreate_with_dimension(dimensions) {
                        Ok(r) => r,
                        Err(SwapchainCreationError::UnsupportedDimensions) => return,
                        Err(e) => panic!("Failed to recreate swapchain: {:?}", e)
                    };

                    swapchain = new_swapchain;
                    if acquire_fullscreen {
                        //Now that the swapchain for fullscreen is created, acquire exclusive mode.
                        swapchain.acquire_full_screen_exclusive_mode().unwrap();
                    }
                    framebuffers = window_size_dependent_setup(&new_images, render_pass.clone(), &mut dynamic_state);
                    recreate_swapchain = false;
                }
                // after which the function call will return an error.
                let (image_num, suboptimal, acquire_future) = match swapchain::acquire_next_image(swapchain.clone(), None) {
                    Ok(r) => r,
                    Err(AcquireError::OutOfDate) => {
                        recreate_swapchain = true;
                        return;
                    },
                    Err(e) => panic!("Failed to acquire next image: {:?}", e)
                };
                if suboptimal {
                    recreate_swapchain = true;
                }
                let clear_values = vec!([0.0, 0.0, 1.0, 1.0].into());
                let command_buffer = AutoCommandBufferBuilder::primary_one_time_submit(device.clone(), queue.family()).unwrap()
                    .begin_render_pass(framebuffers[image_num].clone(), false, clear_values).unwrap()
                    .draw(pipeline.clone(), &dynamic_state, vertex_buffer.clone(), (), ()).unwrap()
                    .end_render_pass().unwrap()
                    .build().unwrap();

                let future = previous_frame_end.take().unwrap()
                    .join(acquire_future)
                    .then_execute(queue.clone(), command_buffer).unwrap()
                    .then_swapchain_present(queue.clone(), swapchain.clone(), image_num)
                    .then_signal_fence_and_flush();

                match future {
                    Ok(future) => {
                        previous_frame_end = Some(Box::new(future) as Box<_>);
                    },
                    Err(FlushError::OutOfDate) => {
                        recreate_swapchain = true;
                        previous_frame_end = Some(Box::new(sync::now(device.clone())) as Box<_>);
                    }
                    Err(e) => {
                        println!("Failed to flush future: {:?}", e);
                        previous_frame_end = Some(Box::new(sync::now(device.clone())) as Box<_>);
                    }
                }
            },
            _ => ()
        }
    });
}

/// This method is called once during initialization, then again whenever the window is resized
fn window_size_dependent_setup(
    images: &[Arc<SwapchainImage<Window>>],
    render_pass: Arc<dyn RenderPassAbstract + Send + Sync>,
    dynamic_state: &mut DynamicState
) -> Vec<Arc<dyn FramebufferAbstract + Send + Sync>> {
    let dimensions = images[0].dimensions();

    let viewport = Viewport {
        origin: [0.0, 0.0],
        dimensions: [dimensions[0] as f32, dimensions[1] as f32],
        depth_range: 0.0 .. 1.0,
    };
    dynamic_state.viewports = Some(vec!(viewport));

    images.iter().map(|image| {
        Arc::new(
            Framebuffer::start(render_pass.clone())
                .add(image.clone()).unwrap()
                .build().unwrap()
        ) as Arc<dyn FramebufferAbstract + Send + Sync>
    }).collect::<Vec<_>>()
}
