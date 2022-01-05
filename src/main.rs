use std::io;
use std::os::raw::c_ulong;
use std::ptr::null;
use x11::xlib::{CurrentTime, Display, XCloseDisplay, XOpenDisplay, XRootWindow};
use x11::xrandr::{
    RR_Rotate_0, XRRConfigCurrentConfiguration, XRRFreeScreenConfigInfo, XRRFreeScreenResources,
    XRRGetScreenInfo, XRRGetScreenResources, XRRRates, XRRRootToScreen, XRRScreenConfiguration,
    XRRScreenResources, XRRSetScreenConfigAndRate, XRRSizes,
    XRRModeInfo,
};

#[derive(Debug)]
struct DisplayInfo {
    display: *mut Display,
    root_window: c_ulong,
}

impl DisplayInfo {
    fn from_primary() -> io::Result<DisplayInfo> {
        let display = unsafe { XOpenDisplay(null()) };
        if display.is_null() {
            return Err(io::ErrorKind::NotFound.into());
        }

        let root_window = unsafe { XRootWindow(display, 0) };
        if root_window == 0 {
            return Err(io::ErrorKind::Other.into());
        }

        if display.is_null() {
            Err(io::ErrorKind::NotFound.into())
        } else {
            Ok(DisplayInfo {
                display,
                root_window,
            })
        }
    }
}

impl Drop for DisplayInfo {
    fn drop(&mut self) {
        if !self.display.is_null() {
            let res = unsafe { XCloseDisplay(self.display) };
            if res != 0 {
                eprintln!("Failed to drop display");
            }
        }
    }
}

#[derive(Debug)]
struct ScreenInfo {
    conf: *mut XRRScreenConfiguration,
}

impl ScreenInfo {
    fn from_display(d: &DisplayInfo) -> io::Result<ScreenInfo> {
        let conf = unsafe { XRRGetScreenInfo(d.display, d.root_window) };
        if conf.is_null() {
            Err(io::ErrorKind::NotFound.into())
        } else {
            Ok(ScreenInfo { conf })
        }
    }
}

impl Drop for ScreenInfo {
    fn drop(&mut self) {
        if !self.conf.is_null() {
            unsafe {
                XRRFreeScreenConfigInfo(self.conf);
            }
        }
    }
}

#[derive(Debug)]
struct ScreenResources {
    res: *mut XRRScreenResources,
}

impl ScreenResources {
    fn from_display(d: &DisplayInfo) -> io::Result<ScreenResources> {
        let res = unsafe { XRRGetScreenResources(d.display, d.root_window) };
        if res.is_null() {
            Err(io::ErrorKind::NotFound.into())
        } else {
            Ok(ScreenResources {
                res
            })
        }
    }

    fn num_modes(&self) -> usize {
        if !self.res.is_null() {
            unsafe { (*self.res).nmode as usize }
        } else {
            0
        }
    }

    fn mode_info_get(&self, index: usize) -> io::Result<XRRModeInfo> {
        if index >= self.num_modes() {
            Err(io::ErrorKind::InvalidInput.into())
        } else {
            Ok(unsafe { *(*self.res).modes.add(index) })
        }
    }
}

impl Drop for ScreenResources {
    fn drop(&mut self) {
        if !self.res.is_null() {
            unsafe {
                XRRFreeScreenResources(self.res);
            }
        }
    }
}

#[derive(Debug)]
struct ScreenSize {
    width: u32,
    height: u32,
    // X11 size index reference needed when fetching freq for a size
    size_index: i32,
}

#[derive(Debug)]
struct RefreshRate {
    freq: u32,
    // X11 freq index reference needed when setting freq
    freq_index: i16,
}

fn get_sizes(d: &DisplayInfo) -> io::Result<Vec<ScreenSize>> {
    let mut num_sizes = 0;
    let mut safe_sizes = Vec::new();

    let screen = unsafe { XRRRootToScreen(d.display, d.root_window) };

    let sizes = unsafe { XRRSizes(d.display, screen, &mut num_sizes) };
    if sizes.is_null() {
        return Err(io::ErrorKind::NotFound.into());
    }

    for i in 0..num_sizes {
        let size = unsafe { *sizes.offset(i as isize) };

        match (u32::try_from(size.width), u32::try_from(size.height)) {
            (Ok(width), Ok(height)) => safe_sizes.push(ScreenSize {
                width,
                height,
                size_index: i,
            }),
            _ => return Err(io::ErrorKind::Other.into()),
        }
    }

    Ok(safe_sizes)
}

fn get_freqs_by_screen_size(d: &DisplayInfo, ssz: &ScreenSize) -> io::Result<Vec<RefreshRate>> {
    let screen = unsafe { XRRRootToScreen(d.display, d.root_window) };

    let mut num_freqs = 0;
    let freqs = unsafe { XRRRates(d.display, screen, ssz.size_index, &mut num_freqs) };
    if freqs.is_null() || num_freqs == 0 {
        return Err(io::ErrorKind::NotFound.into());
    }

    let mut safe_freqs = Vec::new();
    for i in 0..num_freqs {
        let freq = unsafe { *freqs.offset(i as isize) };
        // Safety: Non-null access, converting unsigned shorts to unsigned ints
        safe_freqs.push(freq);
    }

    let screen_resources = ScreenResources::from_display(d)?;

    let num_modes = screen_resources.num_modes();

    let mut refresh_rates = Vec::new();
    let mut freq_index = 0;
    for i in 0..num_modes {
        let mode_info = screen_resources.mode_info_get(i)?;
        let refresh_rate = (mode_info.dotClock
            / (mode_info.hTotal as u64 * mode_info.vTotal as u64))
            as u32;

        if mode_info.width == ssz.width && mode_info.height == ssz.height {
            #[cfg(debug_assertions)]
            dbg!((
                mode_info.id,
                mode_info.width,
                mode_info.height,
                refresh_rate,
                freq_index,
                safe_freqs[freq_index],
            ));

            if freq_index >= safe_freqs.len() {
                return Err(io::ErrorKind::InvalidData.into());
            }

            refresh_rates.push(RefreshRate {
                freq: refresh_rate,
                freq_index: safe_freqs[freq_index],
            });
            freq_index += 1;
        }
    }

    Ok(refresh_rates)
}

fn main() -> io::Result<()> {
    let d = DisplayInfo::from_primary()?;

    let mut sizes = get_sizes(&d)?;
    if sizes.is_empty() {
        eprintln!("Unable to fetch supported screen resolutions");
        return Err(io::ErrorKind::Unsupported.into());
    }

    sizes.sort_by(|a, b| {
        let a_size = a.width * a.height;
        let b_size = b.width * b.height;
        b_size.cmp(&a_size)
    });

    let mut freqs = get_freqs_by_screen_size(&d, &sizes[0])?;
    freqs.sort_by(|a, b| b.freq.cmp(&a.freq));

    let max_size = &sizes[0];
    let max_freq = &freqs[0];

    #[cfg(debug_assertions)]
    dbg!(max_size, max_freq);

    let screen_info = ScreenInfo::from_display(&d)?;

    /* Change screen resolution and refresh rate */
    unsafe {
        let mut rotation = RR_Rotate_0 as u16;
        let _ = XRRConfigCurrentConfiguration(screen_info.conf, &mut rotation);
        XRRSetScreenConfigAndRate(
            d.display,
            screen_info.conf,
            d.root_window,
            max_size.size_index,
            rotation,
            max_freq.freq_index,
            CurrentTime,
        );
    }

    Ok(())
}
