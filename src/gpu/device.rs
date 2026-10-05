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
