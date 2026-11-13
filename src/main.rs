// HeMiPad – a fast, simple text editor: the classic Notepad in the look of Windows 11.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod doc;
mod i18n;
mod print;
mod settings;
mod shell_menu;
mod spell;
mod syntax;
mod sys;
mod theme;
mod widgets;

use eframe::egui;
use std::path::PathBuf;

fn main() -> eframe::Result {
    // the file to open: "HeMiPad.exe file" (also how File Explorer starts it with Open with)
    let file: Option<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).find(|a| !a.as_os_str().is_empty());
    let s = settings::Settings::load();
    i18n::set(s.language);
    shell_menu::repair();

    let icon = egui::IconData { rgba: include_bytes!("../assets/HeMiPad-64.rgba").to_vec(), width: 64, height: 64 };
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("HeMiPad")
        .with_app_id("HeMiPad")
        .with_inner_size([s.width, s.height])
        .with_min_inner_size([360.0, 240.0])
        .with_maximized(s.maximized)
        .with_icon(icon)
        .with_drag_and_drop(true)
        // transparent, so the Mica of Windows 11 shows around the text
        .with_transparent(true);
    // where the window was last time – only when that place is still on a monitor
    let pos = s.position.filter(|&(x, y)| on_screen(x * s.scale, y * s.scale));
    if let Some((x, y)) = pos {
        viewport = viewport.with_position([x, y]);
    }
    let options = eframe::NativeOptions { viewport, centered: pos.is_none(), ..Default::default() };
    eframe::run_native("HeMiPad", options, Box::new(move |cc| Ok(Box::new(app::HeMiPadApp::new(cc, file, s)))))
}

/// True when the top left corner of the title bar (screen pixels) is on a monitor.
fn on_screen(x: f32, y: f32) -> bool {
    use windows_sys::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONULL, MonitorFromPoint};
    let p = windows_sys::Win32::Foundation::POINT { x: x as i32 + 80, y: y as i32 + 16 };
    !unsafe { MonitorFromPoint(p, MONITOR_DEFAULTTONULL) }.is_null()
}
