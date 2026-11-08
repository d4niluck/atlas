use serde_json::Value;
use std::sync::{Mutex, atomic::{AtomicBool, Ordering}};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

const QUICK: &str = "quick-search";
const NARROW_WIDTH: f64 = 440.;
const WIDE_WIDTH: f64 = 760.;
const HEIGHT: f64 = 680.;

#[derive(Default)]
struct MicrophonePrompt(AtomicBool);
impl MicrophonePrompt {
    fn set(&self, active:bool){self.0.store(active,Ordering::SeqCst)}
    fn hide_on_blur(&self)->bool{!self.0.load(Ordering::SeqCst)}
}
static MICROPHONE_PROMPT:MicrophonePrompt=MicrophonePrompt(AtomicBool::new(false));

#[tauri::command]
pub fn voice_microphone_prompt(window:tauri::WebviewWindow,active:bool){
    if window.label()==QUICK {MICROPHONE_PROMPT.set(active)}
}

#[derive(Default)]
pub struct PendingResult(Mutex<Option<Value>>, AtomicBool, AtomicBool, Mutex<Option<(f64, f64)>>, AtomicBool);

// Physical coordinates prevent scaling errors when the menu is on a secondary display.
fn popup_size(wide: bool, scale: f64, available_width: f64, available_height: f64) -> (f64, f64) {
    ((if wide { WIDE_WIDTH } else { NARROW_WIDTH } * scale).min(available_width), (HEIGHT * scale).min(available_height))
}
fn popup_position(x: f64, bottom: f64, width: f64, height: f64, bounds: (f64, f64, f64, f64)) -> (f64, f64) {
    let (left, top, right, lower) = bounds;
    ((x - width / 2.).max(left).min((right - width).max(left)), bottom.max(top).min((lower - height).max(top)))
}
fn position_popup(app: &AppHandle, window: &tauri::WebviewWindow, anchor: (f64, f64), wide: bool) -> Result<(), String> {
    let monitors=app.available_monitors().map_err(|e|e.to_string())?;
    let monitor=monitors.iter().find(|m| {let p=m.position();let s=m.size();anchor.0>=p.x as f64&&anchor.0<(p.x as f64+s.width as f64)&&anchor.1>=p.y as f64&&anchor.1<=(p.y as f64+s.height as f64)}).ok_or("Экран для окна поиска недоступен")?;
    let area=monitor.work_area();let p=area.position;let s=area.size;
    let (width,height)=popup_size(wide,monitor.scale_factor(),s.width as f64,s.height as f64);
    let (x,y)=popup_position(anchor.0,anchor.1,width,height,(p.x as f64,p.y as f64,p.x as f64+s.width as f64,p.y as f64+s.height as f64));
    window.set_size(tauri::PhysicalSize::new(width.round() as u32,height.round() as u32)).map_err(|e|e.to_string())?;
    window.set_position(tauri::PhysicalPosition::new(x.round() as i32,y.round() as i32)).map_err(|e|e.to_string())
}
#[tauri::command]
pub fn set_quick_search_mode(app: AppHandle, wide: bool) -> Result<(), String> {
    let state=app.state::<PendingResult>();
    let anchor=state.3.lock().map_err(|_|"Не удалось изменить размер окна")?;
    state.4.store(wide,Ordering::SeqCst);
    if let (Some(anchor),Some(window))=(*anchor,app.get_webview_window(QUICK)) {position_popup(&app,&window,anchor,wide)?;}
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn microphone_permission_keeps_menu_open_until_request_finishes() {
        let state=MicrophonePrompt::default();
        assert!(state.hide_on_blur());
        state.set(true);
        assert!(!state.hide_on_blur());
        state.set(false);
        assert!(state.hide_on_blur());
    }
    #[test] fn keeps_popup_on_monitor_near_edges() {
        assert_eq!(popup_position(1980., 30., 1520., 1360., (0., 0., 2000., 1600.)), (480., 30.));
        assert_eq!(popup_position(10., 30., 1520., 1360., (0., 0., 2000., 1600.)), (0., 30.));
        assert_eq!(popup_position(-10., 30., 760., 680., (-1200., 0., 0., 900.)), (-760., 30.));
    }
    #[test] fn popup_dimensions_follow_search_mode_and_screen() {
        assert_eq!(popup_size(false, 2., 2000., 1600.), (880., 1360.));
        assert_eq!(popup_size(true, 2., 2000., 1600.), (1520., 1360.));
        assert_eq!(popup_size(true, 2., 1000., 1000.), (1000., 1000.));
    }
    #[test] fn popup_remains_on_screen_when_resized() {
        assert_eq!(popup_position(1980., 30., 1520., 1360., (0., 0., 2000., 1600.)), (480., 30.));
        assert_eq!(popup_position(10., 30., 880., 1360., (0., 0., 2000., 1600.)), (0., 30.));
    }
    #[test] fn exit_waits_for_active_downloads() {
        let exited=std::cell::Cell::new(false);
        let result=exit_after_download_check(||crate::ensure_downloads_finished(&["downloading:42".into()]),||exited.set(true));
        assert!(result.is_err());
        assert!(!exited.get());
        exit_after_download_check(||crate::ensure_downloads_finished(&["complete".into()]),||exited.set(true)).unwrap();
        assert!(exited.get());
    }
    #[test] fn settings_request_is_consumed_once() {
        let state=PendingResult::default();
        state.2.store(true, Ordering::SeqCst);
        assert!(state.2.swap(false, Ordering::SeqCst));
        assert!(!state.2.swap(false, Ordering::SeqCst));
    }
    #[test] fn pending_result_is_consumed_once() {
        let state=PendingResult::default();
        *state.0.lock().unwrap()=Some(serde_json::json!({"workspaceId":"w","id":"i"}));
        assert!(state.0.lock().unwrap().take().is_some());
        assert!(state.0.lock().unwrap().take().is_none());
    }
}

#[tauri::command]
pub fn show_main_window(app: AppHandle) -> Result<(), String> {
    let main=app.get_webview_window("main").ok_or("Основное окно Atlas недоступно")?;
    main.show().map_err(|e|e.to_string())?;
    main.unminimize().map_err(|e|e.to_string())?;
    main.set_focus().map_err(|e|e.to_string())?;
    Ok(())
}
#[tauri::command]
pub fn hide_quick_search(app: AppHandle) -> Result<(), String> {
    app.get_webview_window(QUICK).ok_or("Окно поиска недоступно")?.hide().map_err(|e|e.to_string())
}
#[tauri::command]
pub fn open_quick_result(app: AppHandle, workspace_id: String, id: String, chunk: Value) -> Result<(), String> {
    if workspace_id.is_empty() || workspace_id.len()>256 || id.is_empty() || id.len()>256 || chunk.to_string().len()>262_144 { return Err("Некорректный результат поиска".into()); }
    let state=app.state::<PendingResult>();
    *state.0.lock().map_err(|_|"Не удалось открыть результат")?=Some(serde_json::json!({"workspaceId":workspace_id,"id":id,"chunk":chunk}));
    show_main_window(app.clone())?;
    let _=app.emit_to("main", "atlas-quick-result", ());
    if let Some(window)=app.get_webview_window(QUICK) { let _=window.hide(); }
    Ok(())
}
#[tauri::command]
pub fn open_search_settings(app: AppHandle) -> Result<(), String> {
    app.state::<PendingResult>().2.store(true, Ordering::SeqCst);
    show_main_window(app.clone())?;
    app.emit_to("main", "atlas-quick-settings", ()).map_err(|e|e.to_string())
}
#[tauri::command]
pub fn take_search_settings_request(app: AppHandle) -> bool {
    app.state::<PendingResult>().2.swap(false, Ordering::SeqCst)
}

#[tauri::command]
pub fn request_atlas_exit(app: AppHandle) -> Result<(), String> {
    app.state::<PendingResult>().1.store(true, Ordering::SeqCst);
    show_main_window(app.clone())?;
    app.emit_to("main", "atlas-quit-request", ()).map_err(|e|e.to_string())
}
#[tauri::command]
pub fn take_atlas_exit_request(app: AppHandle) -> bool {
    app.state::<PendingResult>().1.swap(false, Ordering::SeqCst)
}
fn exit_after_download_check(check:impl FnOnce()->Result<(),String>, exit:impl FnOnce())->Result<(),String> {
    check()?;
    exit();
    Ok(())
}
#[tauri::command]
pub fn finish_atlas_exit(app: AppHandle) -> Result<(), String> {
    let _guard=crate::DOWNLOAD_GATE.lock().map_err(|_|"Не удалось проверить загрузки")?;
    exit_after_download_check(||crate::check_downloads(&app),||app.exit(0))
}

#[tauri::command]
pub fn take_quick_result(app: AppHandle) -> Result<Option<Value>, String> {
    app.state::<PendingResult>().0.lock().map(|mut pending|pending.take()).map_err(|_|"Не удалось открыть результат".into())
}

fn icon() -> tauri::image::Image<'static> {
    let size=32u32;
    let mut pixels=vec![0u8;(size*size*4) as usize];
    for y in 0..size { for x in 0..size {
        let dx=x as f64-15.5; let dy=y as f64-15.5;
        let r=(dx*dx+dy*dy).sqrt();
        let drawn=(12.0..=14.0).contains(&r) || (r<=12. && (dx.abs()<1. || dy.abs()<1. || ((dx*dx/38.0)+(dy*dy/150.0)-1.).abs()<0.17));
        if drawn { let i=((y*size+x)*4) as usize; pixels[i+3]=255; }
    }}
    tauri::image::Image::new_owned(pixels,size,size)
}

pub fn install(app: &mut tauri::App, origin: String) -> Result<(), Box<dyn std::error::Error>> {
    app.manage(PendingResult::default());
    let app_handle=app.handle().clone();
    TrayIconBuilder::with_id("atlas-search").icon(icon()).icon_as_template(true).tooltip("Поиск Atlas")
      .on_tray_icon_event(move |_tray,event| {
        let TrayIconEvent::Click {position,rect,button:MouseButton::Left,button_state:MouseButtonState::Up,..}=event else{return};
        let window=if let Some(window)=app_handle.get_webview_window(QUICK){window}else{
            let Ok(url)=format!("{origin}/?window=quick-search").parse() else{return};
            let Ok(window)=WebviewWindowBuilder::new(&app_handle,QUICK,WebviewUrl::External(url))
                .title("Поиск Atlas").inner_size(NARROW_WIDTH,HEIGHT).resizable(false).decorations(false).transparent(true).effects(tauri::window::EffectsBuilder::new().effect(tauri::window::Effect::Popover).state(tauri::window::EffectState::Active).radius(18.).build()).shadow(true).skip_taskbar(true).always_on_top(true).visible(false)
                .on_navigation({let origin=origin.clone();move |url|url.scheme()=="http" && url.origin().ascii_serialization()==origin})
                .build() else{return};
            let hiding=window.clone();
            window.on_window_event(move |event| match event {WindowEvent::Focused(false) if MICROPHONE_PROMPT.hide_on_blur()=>{let _=hiding.hide();},WindowEvent::CloseRequested{api,..}=>{api.prevent_close();let _=hiding.hide();},_=>{}});
            window
        };
        if window.is_visible().unwrap_or(false) {let _=window.hide();return}
        if let Ok(monitors)=app_handle.available_monitors() {
            if let Some(monitor)=monitors.iter().find(|m| {let p=m.position();let s=m.size();position.x>=p.x as f64&&position.x<(p.x as f64+s.width as f64)&&position.y>=p.y as f64&&position.y<(p.y as f64+s.height as f64)}) {
                let scale=monitor.scale_factor();
                let rpos=rect.position.to_physical::<f64>(scale);let rsize=rect.size.to_physical::<f64>(scale);
                let anchor=(rpos.x+rsize.width/2.,rpos.y+rsize.height);
                let state=app_handle.state::<PendingResult>();
                if let Ok(mut saved)=state.3.lock(){*saved=Some(anchor);let _=position_popup(&app_handle,&window,anchor,state.4.load(Ordering::SeqCst));};
            }
        }
        let _=window.show();let _=window.set_focus();
      }).build(app)?;
    if let Some(main)=app.get_webview_window("main") {let hiding=main.clone();main.on_window_event(move|event|if let WindowEvent::CloseRequested{api,..}=event{api.prevent_close();let _=hiding.hide();});}
    Ok(())
}
