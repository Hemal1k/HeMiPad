//! Page Setup and Print, as in Notepad: the dialogs of Windows, the text printed with GDI in the font
//! of the editor, the file name at the top and the page number at the bottom.

use crate::i18n::tr;
use crate::sys::{FontChoice, wide};
use std::ptr::{null, null_mut};
use std::sync::Mutex;
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    CLIP_DEFAULT_PRECIS, CreateFontW, DEFAULT_CHARSET, DEFAULT_PITCH, DEFAULT_QUALITY, DeleteDC, DeleteObject,
    GetDeviceCaps, GetTextExtentExPointW, GetTextExtentPoint32W, GetTextMetricsW, HDC, LOGPIXELSX, LOGPIXELSY,
    OUT_DEFAULT_PRECIS, PHYSICALHEIGHT, PHYSICALOFFSETX, PHYSICALOFFSETY, PHYSICALWIDTH, SelectObject, SetBkMode,
    TEXTMETRICW, TRANSPARENT, TextOutW,
};
use windows_sys::Win32::Storage::Xps::{DOCINFOW, EndDoc, EndPage, StartDocW, StartPage};
use windows_sys::Win32::UI::Controls::Dialogs::{
    PAGESETUPDLGW, PD_NOPAGENUMS, PD_NOSELECTION, PD_RETURNDC, PD_SELECTION, PD_USEDEVMODECOPIESANDCOLLATE,
    PRINTDLGW, PSD_INHUNDREDTHSOFMILLIMETERS, PSD_MARGINS, PageSetupDlgW, PrintDlgW,
};

/// True when Windows has at least one printer (local or connected). Without one, the dialogs of Windows
/// would only offer to install a printer, in a window that does not look like HeMiPad.
pub fn has_printer() -> bool {
    use windows_sys::Win32::Graphics::Printing::{EnumPrintersW, PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL};
    let (mut needed, mut count) = (0u32, 0u32);
    unsafe {
        EnumPrintersW(PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS, null(), 4, null_mut(), 0, &mut needed, &mut count);
        if needed == 0 {
            return false;
        }
        let mut buf = vec![0u8; needed as usize];
        EnumPrintersW(PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS, null(), 4, buf.as_mut_ptr(), needed, &mut needed, &mut count) != 0 && count > 0
    }
}

/// The printer and paper chosen in Page Setup, kept for Print (HGLOBALs of Windows).
static DEVICE: Mutex<(usize, usize)> = Mutex::new((0, 0));

/// Page Setup. `margins` = left, top, right, bottom in hundredths of a millimetre; changed when OK.
pub fn page_setup(owner: isize, margins: &mut [i32; 4]) -> bool {
    let mut dev = DEVICE.lock().unwrap();
    let mut ps: PAGESETUPDLGW = unsafe { std::mem::zeroed() };
    ps.lStructSize = std::mem::size_of::<PAGESETUPDLGW>() as u32;
    ps.hwndOwner = owner as HWND;
    ps.hDevMode = dev.0 as _;
    ps.hDevNames = dev.1 as _;
    ps.Flags = PSD_INHUNDREDTHSOFMILLIMETERS | PSD_MARGINS;
    ps.rtMargin = RECT { left: margins[0], top: margins[1], right: margins[2], bottom: margins[3] };
    let ok = unsafe { PageSetupDlgW(&mut ps) } != 0;
    *dev = (ps.hDevMode as usize, ps.hDevNames as usize);
    if ok {
        *margins = [ps.rtMargin.left, ps.rtMargin.top, ps.rtMargin.right, ps.rtMargin.bottom];
    }
    ok
}

/// The Print dialog and the printing. `selection` = the selected text (offered as "Selection").
/// Ok(false) when cancelled.
pub fn print(owner: isize, title: &str, text: &str, selection: Option<&str>, font: &FontChoice, margins: [i32; 4]) -> Result<bool, String> {
    let mut dev = DEVICE.lock().unwrap();
    let mut pd: PRINTDLGW = unsafe { std::mem::zeroed() };
    pd.lStructSize = std::mem::size_of::<PRINTDLGW>() as u32;
    pd.hwndOwner = owner as HWND;
    pd.hDevMode = dev.0 as _;
    pd.hDevNames = dev.1 as _;
    pd.Flags = PD_RETURNDC | PD_USEDEVMODECOPIESANDCOLLATE | PD_NOPAGENUMS | if selection.is_none() { PD_NOSELECTION } else { 0 };
    pd.nCopies = 1;
    let ok = unsafe { PrintDlgW(&mut pd) } != 0;
    *dev = (pd.hDevMode as usize, pd.hDevNames as usize);
    drop(dev);
    if !ok || pd.hDC.is_null() {
        return Ok(false);
    }
    let body = match selection {
        Some(s) if pd.Flags & PD_SELECTION != 0 => s,
        _ => text,
    };
    let r = unsafe { print_to(pd.hDC, title, body, font, margins) };
    unsafe { DeleteDC(pd.hDC) };
    r.map(|_| true)
}

/// Tabs as spaces (tab stops every 8 columns, as Notepad prints).
fn expand_tabs(line: &str) -> Vec<u16> {
    let mut out = Vec::with_capacity(line.len());
    let mut col = 0;
    for c in line.chars() {
        if c == '\t' {
            let n = 8 - col % 8;
            out.extend(std::iter::repeat_n(b' ' as u16, n));
            col += n;
        } else {
            let mut buf = [0u16; 2];
            out.extend_from_slice(c.encode_utf16(&mut buf));
            col += 1;
        }
    }
    out
}

unsafe fn print_to(hdc: HDC, title: &str, text: &str, font: &FontChoice, m: [i32; 4]) -> Result<(), String> {
    unsafe {
        let (dpi_x, dpi_y) = (GetDeviceCaps(hdc, LOGPIXELSX as _), GetDeviceCaps(hdc, LOGPIXELSY as _));
        let (phys_w, phys_h) = (GetDeviceCaps(hdc, PHYSICALWIDTH as _), GetDeviceCaps(hdc, PHYSICALHEIGHT as _));
        let (off_x, off_y) = (GetDeviceCaps(hdc, PHYSICALOFFSETX as _), GetDeviceCaps(hdc, PHYSICALOFFSETY as _));
        let mm = |v: i32, dpi: i32| (v as i64 * dpi as i64 / 2540) as i32;
        let left = (mm(m[0], dpi_x) - off_x).max(0);
        let right = phys_w - mm(m[2], dpi_x) - off_x;
        let top = (mm(m[1], dpi_y) - off_y).max(0);
        let bottom = phys_h - mm(m[3], dpi_y) - off_y;
        if right - left < 50 || bottom - top < 50 {
            return Err(tr("The margins leave no room for text.", "Okraje nenechávají místo pro text.").into());
        }

        let face = wide(&font.face);
        let height = -((font.size * dpi_y as f32 / 72.0).round() as i32);
        let hfont = CreateFontW(
            height,
            0,
            0,
            0,
            if font.bold { 700 } else { 400 },
            font.italic as u32,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            DEFAULT_QUALITY as u32,
            DEFAULT_PITCH as u32,
            face.as_ptr(),
        );
        let old = SelectObject(hdc, hfont as _);
        SetBkMode(hdc, TRANSPARENT as _);
        let mut tm: TEXTMETRICW = std::mem::zeroed();
        GetTextMetricsW(hdc, &mut tm);
        let lh = (tm.tmHeight + tm.tmExternalLeading).max(1);

        let doc_name = wide(title);
        let di = DOCINFOW { cbSize: std::mem::size_of::<DOCINFOW>() as i32, lpszDocName: doc_name.as_ptr(), lpszOutput: null(), lpszDatatype: null(), fwType: 0 };
        if StartDocW(hdc, &di) <= 0 {
            SelectObject(hdc, old);
            DeleteObject(hfont as _);
            return Err(tr("Printing did not start.", "Tisk se nespustil.").into());
        }

        let body_top = top + lh * 2;
        let body_bottom = bottom - lh * 2;
        let width = right - left;
        let mut page = 0;
        let mut y = i32::MAX;
        let centered = |s: &[u16], y: i32| {
            let mut sz = std::mem::zeroed();
            GetTextExtentPoint32W(hdc, s.as_ptr(), s.len() as i32, &mut sz);
            TextOutW(hdc, left + ((width - sz.cx) / 2).max(0), y, s.as_ptr(), s.len() as i32);
        };
        let title_w: Vec<u16> = title.encode_utf16().collect();
        let new_page = |page: &mut i32, y: &mut i32| {
            if *page > 0 {
                EndPage(hdc);
            }
            StartPage(hdc);
            *page += 1;
            centered(&title_w, top);
            let foot: Vec<u16> = tr("Page {n}", "Stránka {n}").replace("{n}", &page.to_string()).encode_utf16().collect();
            centered(&foot, bottom - lh);
            *y = body_top;
        };

        for line in text.split('\n') {
            let mut w = expand_tabs(line);
            loop {
                if y + lh > body_bottom {
                    new_page(&mut page, &mut y);
                }
                if w.is_empty() {
                    y += lh;
                    break;
                }
                let mut fit = 0i32;
                let mut sz = std::mem::zeroed();
                GetTextExtentExPointW(hdc, w.as_ptr(), w.len() as i32, width, &mut fit, null_mut(), &mut sz);
                let mut n = (fit.max(1) as usize).min(w.len());
                if n < w.len() {
                    // break after the last space that fits
                    if let Some(sp) = w[..n].iter().rposition(|&c| c == b' ' as u16)
                        && sp > 0
                    {
                        n = sp + 1;
                    }
                }
                TextOutW(hdc, left, y, w.as_ptr(), n as i32);
                y += lh;
                w.drain(..n);
                if w.is_empty() {
                    break;
                }
            }
        }
        if page == 0 {
            new_page(&mut page, &mut y);
        }
        EndPage(hdc);
        EndDoc(hdc);
        SelectObject(hdc, old);
        DeleteObject(hfont as _);
        Ok(())
    }
}
