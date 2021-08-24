// Copyright 2018-2021 the Deno authors. All rights reserved. MIT license.

use deno_core::error::AnyError;
use deno_core::ResourceId;
use deno_core::{OpState, Resource};
use serde::Deserialize;
use std::borrow::Cow;
use std::cell::RefCell;
use std::num::NonZeroU32;

use crate::texture::GpuTextureAspect;

use super::error::WebGpuResult;

pub(crate) struct WebGpuCommandEncoder(pub(crate) wgpu_core::id::CommandEncoderId);
impl Resource for WebGpuCommandEncoder {
    fn name(&self) -> Cow<str> {
        "webGPUCommandEncoder".into()
    }
}

pub(crate) struct WebGpuCommandBuffer(pub(crate) wgpu_core::id::CommandBufferId);
impl Resource for WebGpuCommandBuffer {
    fn name(&self) -> Cow<str> {
        "webGPUCommandBuffer".into()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCommandEncoderArgs {
    device_rid: ResourceId,
    label: Option<String>,
    _measure_execution_time: Option<bool>, // not yet implemented
}

pub fn op_webgpu_create_command_encoder(
    state: &mut OpState,
    args: CreateCommandEncoderArgs,
    _: (),
) -> Result<WebGpuResult, AnyError> {
    let instance = state.borrow::<super::Instance>();
    let device_resource = state
        .resource_table
        .get::<super::WebGpuDevice>(args.device_rid)?;
    let device = device_resource.0;

    let descriptor = wgpu_types::CommandEncoderDescriptor {
        label: args.label.map(Cow::from),
    };

    gfx_put!(device => instance.device_create_command_encoder(
    device,
    &descriptor,
    std::marker::PhantomData
  ) => state, WebGpuCommandEncoder)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuRenderPassColorAttachment {
    view: ResourceId,
    resolve_target: Option<ResourceId>,
    load_op: GpuLoadOp<super::render_pass::GpuColor>,
    store_op: GpuStoreOp,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum GpuLoadOp<T> {
    Load,
    Clear(T),
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum GpuStoreOp {
    Store,
    Discard,
}

impl From<GpuStoreOp> for wgpu_core::command::StoreOp {
    fn from(value: GpuStoreOp) -> wgpu_core::command::StoreOp {
        match value {
            GpuStoreOp::Store => wgpu_core::command::StoreOp::Store,
            GpuStoreOp::Discard => wgpu_core::command::StoreOp::Discard,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GpuRenderPassDepthStencilAttachment {
    view: ResourceId,
    depth_load_op: GpuLoadOp<f32>,
    depth_store_op: GpuStoreOp,
    depth_read_only: bool,
    stencil_load_op: GpuLoadOp<u32>,
    stencil_store_op: GpuStoreOp,
    stencil_read_only: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandEncoderBeginRenderPassArgs {
    command_encoder_rid: ResourceId,
    label: Option<String>,
    color_attachments: Vec<GpuRenderPassColorAttachment>,
    depth_stencil_attachment: Option<GpuRenderPassDepthStencilAttachment>,
    _occlusion_query_set: Option<u32>, // not yet implemented
}

pub fn op_webgpu_command_encoder_begin_render_pass(
    state: &mut OpState,
    args: CommandEncoderBeginRenderPassArgs,
    _: (),
) -> Result<WebGpuResult, AnyError> {
    let command_encoder_resource = state
        .resource_table
        .get::<WebGpuCommandEncoder>(args.command_encoder_rid)?;

    let mut color_attachments = vec![];

    for color_attachment in args.color_attachments {
        let texture_view_resource = state
            .resource_table
            .get::<super::texture::WebGpuTextureView>(color_attachment.view)?;

        let resolve_target = color_attachment
            .resolve_target
            .map(|rid| {
                state
                    .resource_table
                    .get::<super::texture::WebGpuTextureView>(rid)
            })
            .transpose()?
            .map(|texture| texture.0);

        let attachment = wgpu_core::command::RenderPassColorAttachment {
            view: texture_view_resource.0,
            resolve_target,
            channel: match color_attachment.load_op {
                GpuLoadOp::Load => wgpu_core::command::PassChannel {
                    load_op: wgpu_core::command::LoadOp::Load,
                    store_op: color_attachment.store_op.into(),
                    clear_value: Default::default(),
                    read_only: false,
                },
                GpuLoadOp::Clear(color) => wgpu_core::command::PassChannel {
                    load_op: wgpu_core::command::LoadOp::Clear,
                    store_op: color_attachment.store_op.into(),
                    clear_value: wgpu_types::Color {
                        r: color.r,
                        g: color.g,
                        b: color.b,
                        a: color.a,
                    },
                    read_only: false,
                },
            },
        };

        color_attachments.push(attachment)
    }

    let mut depth_stencil_attachment = None;

    if let Some(attachment) = args.depth_stencil_attachment {
        let texture_view_resource = state
            .resource_table
            .get::<super::texture::WebGpuTextureView>(attachment.view)?;

        depth_stencil_attachment = Some(wgpu_core::command::RenderPassDepthStencilAttachment {
            view: texture_view_resource.0,
            depth: match attachment.depth_load_op {
                GpuLoadOp::Load => wgpu_core::command::PassChannel {
                    load_op: wgpu_core::command::LoadOp::Load,
                    store_op: attachment.depth_store_op.into(),
                    clear_value: 0.0,
                    read_only: attachment.depth_read_only,
                },
                GpuLoadOp::Clear(value) => wgpu_core::command::PassChannel {
                    load_op: wgpu_core::command::LoadOp::Clear,
                    store_op: attachment.depth_store_op.into(),
                    clear_value: value,
                    read_only: attachment.depth_read_only,
                },
            },
            stencil: match attachment.stencil_load_op {
                GpuLoadOp::Load => wgpu_core::command::PassChannel {
                    load_op: wgpu_core::command::LoadOp::Load,
                    store_op: attachment.stencil_store_op.into(),
                    clear_value: 0,
                    read_only: attachment.stencil_read_only,
                },
                GpuLoadOp::Clear(value) => wgpu_core::command::PassChannel {
                    load_op: wgpu_core::command::LoadOp::Clear,
                    store_op: attachment.stencil_store_op.into(),
                    clear_value: value,
                    read_only: attachment.stencil_read_only,
                },
            },
        });
    }

    let descriptor = wgpu_core::command::RenderPassDescriptor {
        label: args.label.map(Cow::from),
        color_attachments: Cow::from(color_attachments),
        depth_stencil_attachment: depth_stencil_attachment.as_ref(),
    };

    let render_pass = wgpu_core::command::RenderPass::new(command_encoder_resource.0, &descriptor);

    let rid = state
        .resource_table
        .add(super::render_pass::WebGpuRenderPass(RefCell::new(
            render_pass,
        )));

    Ok(WebGpuResult::rid(rid))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandEncoderBeginComputePassArgs {
    command_encoder_rid: ResourceId,
    label: Option<String>,
}

pub fn op_webgpu_command_encoder_begin_compute_pass(
    state: &mut OpState,
    args: CommandEncoderBeginComputePassArgs,
    _: (),
) -> Result<WebGpuResult, AnyError> {
    let command_encoder_resource = state
        .resource_table
        .get::<WebGpuCommandEncoder>(args.command_encoder_rid)?;

    let descriptor = wgpu_core::command::ComputePassDescriptor {
        label: args.label.map(Cow::from),
    };

    let compute_pass =
        wgpu_core::command::ComputePass::new(command_encoder_resource.0, &descriptor);

    let rid = state
        .resource_table
        .add(super::compute_pass::WebGpuComputePass(RefCell::new(
            compute_pass,
        )));

    Ok(WebGpuResult::rid(rid))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandEncoderCopyBufferToBufferArgs {
    command_encoder_rid: ResourceId,
    source: ResourceId,
    source_offset: u64,
    destination: ResourceId,
    destination_offset: u64,
    size: u64,
}

pub fn op_webgpu_command_encoder_copy_buffer_to_buffer(
    state: &mut OpState,
    args: CommandEncoderCopyBufferToBufferArgs,
    _: (),
) -> Result<WebGpuResult, AnyError> {
    let instance = state.borrow::<super::Instance>();
    let command_encoder_resource = state
        .resource_table
        .get::<WebGpuCommandEncoder>(args.command_encoder_rid)?;
    let command_encoder = command_encoder_resource.0;
    let source_buffer_resource = state
        .resource_table
        .get::<super::buffer::WebGpuBuffer>(args.source)?;
    let source_buffer = source_buffer_resource.0;
    let destination_buffer_resource = state
        .resource_table
        .get::<super::buffer::WebGpuBuffer>(args.destination)?;
    let destination_buffer = destination_buffer_resource.0;

    gfx_ok!(command_encoder => instance.command_encoder_copy_buffer_to_buffer(
      command_encoder,
      source_buffer,
      args.source_offset,
      destination_buffer,
      args.destination_offset,
      args.size
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuImageCopyBuffer {
    buffer: ResourceId,
    offset: u64,
    bytes_per_row: Option<u32>,
    rows_per_image: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuOrigin3D {
    pub x: u32,
    pub y: u32,
    pub z: u32,
}

impl From<GpuOrigin3D> for wgpu_types::Origin3d {
    fn from(origin: GpuOrigin3D) -> wgpu_types::Origin3d {
        wgpu_types::Origin3d {
            x: origin.x,
            y: origin.y,
            z: origin.z,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuImageCopyTexture {
    pub texture: ResourceId,
    pub mip_level: u32,
    pub origin: GpuOrigin3D,
    pub aspect: GpuTextureAspect,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandEncoderCopyBufferToTextureArgs {
    command_encoder_rid: ResourceId,
    source: GpuImageCopyBuffer,
    destination: GpuImageCopyTexture,
    copy_size: super::texture::GpuExtent3D,
}

pub fn op_webgpu_command_encoder_copy_buffer_to_texture(
    state: &mut OpState,
    args: CommandEncoderCopyBufferToTextureArgs,
    _: (),
) -> Result<WebGpuResult, AnyError> {
    let instance = state.borrow::<super::Instance>();
    let command_encoder_resource = state
        .resource_table
        .get::<WebGpuCommandEncoder>(args.command_encoder_rid)?;
    let command_encoder = command_encoder_resource.0;
    let source_buffer_resource = state
        .resource_table
        .get::<super::buffer::WebGpuBuffer>(args.source.buffer)?;
    let destination_texture_resource = state
        .resource_table
        .get::<super::texture::WebGpuTexture>(args.destination.texture)?;

    let source = wgpu_core::command::ImageCopyBuffer {
        buffer: source_buffer_resource.0,
        layout: wgpu_types::ImageDataLayout {
            offset: args.source.offset,
            bytes_per_row: NonZeroU32::new(args.source.bytes_per_row.unwrap_or(0)),
            rows_per_image: NonZeroU32::new(args.source.rows_per_image.unwrap_or(0)),
        },
    };
    let destination = wgpu_core::command::ImageCopyTexture {
        texture: destination_texture_resource.0,
        mip_level: args.destination.mip_level,
        origin: args.destination.origin.into(),
        aspect: args.destination.aspect.into(),
    };
    gfx_ok!(command_encoder => instance.command_encoder_copy_buffer_to_texture(
      command_encoder,
      &source,
      &destination,
      &args.copy_size.into()
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandEncoderCopyTextureToBufferArgs {
    command_encoder_rid: ResourceId,
    source: GpuImageCopyTexture,
    destination: GpuImageCopyBuffer,
    copy_size: super::texture::GpuExtent3D,
}

pub fn op_webgpu_command_encoder_copy_texture_to_buffer(
    state: &mut OpState,
    args: CommandEncoderCopyTextureToBufferArgs,
    _: (),
) -> Result<WebGpuResult, AnyError> {
    let instance = state.borrow::<super::Instance>();
    let command_encoder_resource = state
        .resource_table
        .get::<WebGpuCommandEncoder>(args.command_encoder_rid)?;
    let command_encoder = command_encoder_resource.0;
    let source_texture_resource = state
        .resource_table
        .get::<super::texture::WebGpuTexture>(args.source.texture)?;
    let destination_buffer_resource = state
        .resource_table
        .get::<super::buffer::WebGpuBuffer>(args.destination.buffer)?;

    let source = wgpu_core::command::ImageCopyTexture {
        texture: source_texture_resource.0,
        mip_level: args.source.mip_level,
        origin: args.source.origin.into(),
        aspect: args.source.aspect.into(),
    };
    let destination = wgpu_core::command::ImageCopyBuffer {
        buffer: destination_buffer_resource.0,
        layout: wgpu_types::ImageDataLayout {
            offset: args.destination.offset,
            bytes_per_row: NonZeroU32::new(args.destination.bytes_per_row.unwrap_or(0)),
            rows_per_image: NonZeroU32::new(args.destination.rows_per_image.unwrap_or(0)),
        },
    };
    gfx_ok!(command_encoder => instance.command_encoder_copy_texture_to_buffer(
      command_encoder,
      &source,
      &destination,
      &args.copy_size.into()
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandEncoderCopyTextureToTextureArgs {
    command_encoder_rid: ResourceId,
    source: GpuImageCopyTexture,
    destination: GpuImageCopyTexture,
    copy_size: super::texture::GpuExtent3D,
}

pub fn op_webgpu_command_encoder_copy_texture_to_texture(
    state: &mut OpState,
    args: CommandEncoderCopyTextureToTextureArgs,
    _: (),
) -> Result<WebGpuResult, AnyError> {
    let instance = state.borrow::<super::Instance>();
    let command_encoder_resource = state
        .resource_table
        .get::<WebGpuCommandEncoder>(args.command_encoder_rid)?;
    let command_encoder = command_encoder_resource.0;
    let source_texture_resource = state
        .resource_table
        .get::<super::texture::WebGpuTexture>(args.source.texture)?;
    let destination_texture_resource = state
        .resource_table
        .get::<super::texture::WebGpuTexture>(args.destination.texture)?;

    let source = wgpu_core::command::ImageCopyTexture {
        texture: source_texture_resource.0,
        mip_level: args.source.mip_level,
        origin: args.source.origin.into(),
        aspect: args.source.aspect.into(),
    };
    let destination = wgpu_core::command::ImageCopyTexture {
        texture: destination_texture_resource.0,
        mip_level: args.destination.mip_level,
        origin: args.destination.origin.into(),
        aspect: args.destination.aspect.into(),
    };
    gfx_ok!(command_encoder => instance.command_encoder_copy_texture_to_texture(
      command_encoder,
      &source,
      &destination,
      &args.copy_size.into()
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandEncoderPushDebugGroupArgs {
    command_encoder_rid: ResourceId,
    group_label: String,
}

pub fn op_webgpu_command_encoder_push_debug_group(
    state: &mut OpState,
    args: CommandEncoderPushDebugGroupArgs,
    _: (),
) -> Result<WebGpuResult, AnyError> {
    let instance = state.borrow::<super::Instance>();
    let command_encoder_resource = state
        .resource_table
        .get::<WebGpuCommandEncoder>(args.command_encoder_rid)?;
    let command_encoder = command_encoder_resource.0;

    gfx_ok!(command_encoder => instance
    .command_encoder_push_debug_group(command_encoder, &args.group_label))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandEncoderPopDebugGroupArgs {
    command_encoder_rid: ResourceId,
}

pub fn op_webgpu_command_encoder_pop_debug_group(
    state: &mut OpState,
    args: CommandEncoderPopDebugGroupArgs,
    _: (),
) -> Result<WebGpuResult, AnyError> {
    let instance = state.borrow::<super::Instance>();
    let command_encoder_resource = state
        .resource_table
        .get::<WebGpuCommandEncoder>(args.command_encoder_rid)?;
    let command_encoder = command_encoder_resource.0;

    gfx_ok!(command_encoder => instance.command_encoder_pop_debug_group(command_encoder))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandEncoderInsertDebugMarkerArgs {
    command_encoder_rid: ResourceId,
    marker_label: String,
}

pub fn op_webgpu_command_encoder_insert_debug_marker(
    state: &mut OpState,
    args: CommandEncoderInsertDebugMarkerArgs,
    _: (),
) -> Result<WebGpuResult, AnyError> {
    let instance = state.borrow::<super::Instance>();
    let command_encoder_resource = state
        .resource_table
        .get::<WebGpuCommandEncoder>(args.command_encoder_rid)?;
    let command_encoder = command_encoder_resource.0;

    gfx_ok!(command_encoder => instance.command_encoder_insert_debug_marker(
      command_encoder,
      &args.marker_label
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandEncoderWriteTimestampArgs {
    command_encoder_rid: ResourceId,
    query_set: ResourceId,
    query_index: u32,
}

pub fn op_webgpu_command_encoder_write_timestamp(
    state: &mut OpState,
    args: CommandEncoderWriteTimestampArgs,
    _: (),
) -> Result<WebGpuResult, AnyError> {
    let instance = state.borrow::<super::Instance>();
    let command_encoder_resource = state
        .resource_table
        .get::<WebGpuCommandEncoder>(args.command_encoder_rid)?;
    let command_encoder = command_encoder_resource.0;
    let query_set_resource = state
        .resource_table
        .get::<super::WebGpuQuerySet>(args.query_set)?;

    gfx_ok!(command_encoder => instance.command_encoder_write_timestamp(
      command_encoder,
      query_set_resource.0,
      args.query_index
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandEncoderResolveQuerySetArgs {
    command_encoder_rid: ResourceId,
    query_set: ResourceId,
    first_query: u32,
    query_count: u32,
    destination: ResourceId,
    destination_offset: u64,
}

pub fn op_webgpu_command_encoder_resolve_query_set(
    state: &mut OpState,
    args: CommandEncoderResolveQuerySetArgs,
    _: (),
) -> Result<WebGpuResult, AnyError> {
    let instance = state.borrow::<super::Instance>();
    let command_encoder_resource = state
        .resource_table
        .get::<WebGpuCommandEncoder>(args.command_encoder_rid)?;
    let command_encoder = command_encoder_resource.0;
    let query_set_resource = state
        .resource_table
        .get::<super::WebGpuQuerySet>(args.query_set)?;
    let destination_resource = state
        .resource_table
        .get::<super::buffer::WebGpuBuffer>(args.destination)?;

    gfx_ok!(command_encoder => instance.command_encoder_resolve_query_set(
      command_encoder,
      query_set_resource.0,
      args.first_query,
      args.query_count,
      destination_resource.0,
      args.destination_offset
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandEncoderFinishArgs {
    command_encoder_rid: ResourceId,
    label: Option<String>,
}

pub fn op_webgpu_command_encoder_finish(
    state: &mut OpState,
    args: CommandEncoderFinishArgs,
    _: (),
) -> Result<WebGpuResult, AnyError> {
    let command_encoder_resource = state
        .resource_table
        .take::<WebGpuCommandEncoder>(args.command_encoder_rid)?;
    let command_encoder = command_encoder_resource.0;
    let instance = state.borrow::<super::Instance>();

    let descriptor = wgpu_types::CommandBufferDescriptor {
        label: args.label.map(Cow::from),
    };

    gfx_put!(command_encoder => instance.command_encoder_finish(
    command_encoder,
    &descriptor
  ) => state, WebGpuCommandBuffer)
}
