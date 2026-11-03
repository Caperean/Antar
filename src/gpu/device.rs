use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{GpuAdapter, GpuDevice, GpuQueue, GpuRequestAdapterOptions};

/// Request a `GpuAdapter` and then a `GpuDevice` from the browser.
///
/// This is async because both operations cross the JS/WASM boundary.
pub async fn init_device() -> Result<(GpuAdapter, GpuDevice, GpuQueue), JsValue> {
    let gpu = web_sys::window()
        .ok_or_else(|| JsValue::from_str("no window"))?
        .navigator()
        .gpu();

    let adapter_opts = GpuRequestAdapterOptions::new();
    let adapter: GpuAdapter = JsFuture::from(gpu.request_adapter_with_options(&adapter_opts))
        .await?
        .dyn_into()?;

    let device_desc = web_sys::GpuDeviceDescriptor::new();
    let device: GpuDevice = JsFuture::from(adapter.request_device_with_descriptor(&device_desc))
        .await?
        .dyn_into()?;

    let queue = device.queue();
    Ok((adapter, device, queue))
}

/// Report WebGPU failures that would otherwise be silent.
///
/// Pipeline and bind-group validation errors do not throw — they surface
/// asynchronously through `onuncapturederror`. Without a handler they vanish,
/// and the observable result is a pipeline that renders nothing. This codebase
/// has already shipped that failure mode once: an oversized vertex layout made
/// `createRenderPipeline` fail invisibly, so sprites simply never drew (see
/// the note in `pipeline.rs`).
pub fn install_error_reporting(device: &GpuDevice) {
    let handler = Closure::wrap(Box::new(|event: web_sys::GpuUncapturedErrorEvent| {
        web_sys::console::error_2(&"WebGPU error".into(), &event.error().message().into());
    }) as Box<dyn FnMut(web_sys::GpuUncapturedErrorEvent)>);

    device.set_onuncapturederror(Some(handler.as_ref().unchecked_ref()));

    // The device holds the only reference to this closure, so it has to outlive
    // every later call into the renderer. Forgetting it is deliberate.
    std::mem::forget(handler);

    // A lost device never recovers on its own. Every subsequent call would fail
    // with no explanation, so log the reason the moment it happens.
    let lost = device.lost();
    wasm_bindgen_futures::spawn_local(async move {
        match JsFuture::from(lost).await {
            Ok(info) => {
                if let Ok(info) = info.dyn_into::<web_sys::GpuDeviceLostInfo>() {
                    web_sys::console::error_2(&"WebGPU device lost".into(), &info.message().into());
                }
            }
            Err(err) => web_sys::console::error_1(&err),
        }
    });
}
