//! Development-only native capture. Uses actual GPUI text shaping and rendering.
use gpui_kit::base::test_support;
use gpui_kit::*;
use serde_json::json;
use std::{path::PathBuf, time::Duration};

pub fn schedule(window: &mut Window, cx: &mut App) {
    let Some(dir) = std::env::var_os("MIO_GPUI_CAPTURE_DIR").map(PathBuf::from) else {
        return;
    };
    let handle = window.window_handle();
    cx.spawn(async move |cx| {
        // Embedded image assets still decode asynchronously. Let those settle before capturing.
        cx.background_executor().timer(Duration::from_millis(if std::env::var_os("MIO_GPUI_CAPTURE_PAGE").is_some() { 1800 } else { 800 })).await;
        let result=cx.update_window(handle,|_,window,cx| {
            window.refresh(); window.draw(cx).clear(cx);
            let size=window.viewport_size();
            let elements:serde_json::Map<_,_>=test_support::snapshots(window).into_iter().filter_map(|entry|{
                let id=entry.path().last()?;
                let ElementId::Name(name)=id else{return None};
                let bounds=entry.bounds();
                Some((name.to_string(),json!({"bounds":{"x":f32::from(bounds.origin.x),"y":f32::from(bounds.origin.y),"width":f32::from(bounds.size.width),"height":f32::from(bounds.size.height)}})))
            }).collect();
            std::fs::create_dir_all(&dir)?;
            let theme=std::env::var("MIO_GPUI_CAPTURE_THEME").unwrap_or_else(|_|"light".into());
            let variant=if std::env::var_os("MIO_GPUI_FIXTURE_PORT").is_some() { "fixture-" } else { "" };
            let page=std::env::var("MIO_GPUI_CAPTURE_PAGE").ok().map(|p|{let c=std::env::var("MIO_GPUI_CAPTURE_CATEGORY").ok().filter(|c|!c.is_empty());match c{Some(c)=>format!("{p}-{c}-"),None=>format!("{p}-")}}).unwrap_or_default();
            let name=format!("gpui-{page}{variant}{theme}-{}x{}",f32::from(size.width) as u32,f32::from(size.height) as u32);
            let value=json!({"theme":theme,"viewport":{"width":f32::from(size.width),"height":f32::from(size.height),"dpr":f32::from(window.scale_factor())},"elements":elements});
            std::fs::write(dir.join(format!("{name}.json")),serde_json::to_vec_pretty(&value)?)?;
            window.render_to_image()?.save(dir.join(format!("{name}.png")))?;
            Ok::<_,anyhow::Error>(())
        });
        match result {Ok(Ok(()))=>{},error=>eprintln!("Native capture failed: {error:?}")};
        let _=cx.update(|cx|cx.quit());
    }).detach();
}
