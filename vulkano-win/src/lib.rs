extern crate vulkano;
extern crate winit;

#[cfg(target_os = "macos")]
extern crate objc;
#[cfg(target_os = "macos")]
extern crate cocoa;
#[cfg(target_os = "macos")]
extern crate metal_rs as metal;

use std::borrow::Borrow;
use std::error;
use std::fmt;
#[cfg(target_os = "windows")]
use std::ptr;
use std::rc::Rc;
use std::sync::Arc;

use vulkano::instance::Instance;
use vulkano::instance::InstanceExtensions;
use vulkano::swapchain::Surface;
use vulkano::swapchain::SurfaceCreationError;
use winit::{EventsLoop, WindowBuilder};
use winit::CreationError as WindowCreationError;

#[cfg(target_os = "macos")]
use cocoa::appkit::{NSView, NSWindow};
#[cfg(target_os = "macos")]
use cocoa::base::id as cocoa_id;
#[cfg(target_os = "macos")]
use metal::CoreAnimationLayer;
#[cfg(target_os = "macos")]
use objc::runtime::YES;

#[cfg(target_os = "macos")]
use std::mem;

pub fn required_extensions() -> InstanceExtensions {
    let ideal = InstanceExtensions {
        khr_surface: true,
        khr_xlib_surface: true,
        khr_xcb_surface: true,
        khr_wayland_surface: true,
        khr_android_surface: true,
        khr_win32_surface: true,
        mvk_ios_surface: true,
        mvk_macos_surface: true,
        ..InstanceExtensions::none()
    };

    match InstanceExtensions::supported_by_core() {
        Ok(supported) => supported.intersection(&ideal),
        Err(_) => InstanceExtensions::none(),
    }
}

/// Create a surface from the window type `W`. The surface borrows the window
/// to prevent it from being dropped before the surface.
pub fn create_vk_surface<W>(
    window: W, instance: Arc<Instance>,
) -> Result<Arc<Surface<W>>, SurfaceCreationError>
where
    W: SafeBorrow<winit::Window>,
{
    unsafe { winit_to_surface(instance, window) }
}

pub trait VkSurfaceBuild {
    fn build_vk_surface(
        self, events_loop: &EventsLoop, instance: Arc<Instance>,
    ) -> Result<Arc<Surface<winit::Window>>, CreationError>;
}

impl VkSurfaceBuild for WindowBuilder {
    fn build_vk_surface(
        self, events_loop: &EventsLoop, instance: Arc<Instance>,
    ) -> Result<Arc<Surface<winit::Window>>, CreationError> {
        let window = self.build(events_loop)?;
        Ok(create_vk_surface(window, instance)?)
    }
}

/// Error that can happen when creating a window.
#[derive(Debug)]
pub enum CreationError {
    /// Error when creating the surface.
    SurfaceCreationError(SurfaceCreationError),
    /// Error when creating the window.
    WindowCreationError(WindowCreationError),
}

impl error::Error for CreationError {
    #[inline]
    fn description(&self) -> &str {
        match *self {
            CreationError::SurfaceCreationError(_) => "error while creating the surface",
            CreationError::WindowCreationError(_) => "error while creating the window",
        }
    }

    #[inline]
    fn cause(&self) -> Option<&error::Error> {
        match *self {
            CreationError::SurfaceCreationError(ref err) => Some(err),
            CreationError::WindowCreationError(ref err) => Some(err),
        }
    }
}

impl fmt::Display for CreationError {
    #[inline]
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(fmt, "{}", error::Error::description(self))
    }
}

impl From<SurfaceCreationError> for CreationError {
    #[inline]
    fn from(err: SurfaceCreationError) -> CreationError {
        CreationError::SurfaceCreationError(err)
    }
}

impl From<WindowCreationError> for CreationError {
    #[inline]
    fn from(err: WindowCreationError) -> CreationError {
        CreationError::WindowCreationError(err)
    }
}

#[cfg(target_os = "android")]
unsafe fn winit_to_surface<W: SafeBorrow<winit::Window>>(
    instance: Arc<Instance>, win: W,
) -> Result<Arc<Surface<W>>, SurfaceCreationError> {
    use winit::os::android::WindowExt;
    Surface::from_anativewindow(instance, win.borrow().get_native_window(), win)
}

#[cfg(all(unix, not(target_os = "android"), not(target_os = "macos")))]
unsafe fn winit_to_surface<W: SafeBorrow<winit::Window>>(
    instance: Arc<Instance>, win: W,
) -> Result<Arc<Surface<W>>, SurfaceCreationError> {
    use winit::os::unix::WindowExt;
    match (
        win.borrow().get_wayland_display(),
        win.borrow().get_wayland_surface(),
    ) {
        (Some(display), Some(surface)) => Surface::from_wayland(instance, display, surface, win),
        _ => {
            // No wayland display found, check if we can use xlib.
            // If not, we use xcb.
            if instance.loaded_extensions().khr_xlib_surface {
                Surface::from_xlib(
                    instance,
                    win.borrow().get_xlib_display().unwrap(),
                    win.borrow().get_xlib_window().unwrap() as _,
                    win,
                )
            } else {
                Surface::from_xcb(
                    instance,
                    win.borrow().get_xcb_connection().unwrap(),
                    win.borrow().get_xlib_window().unwrap() as _,
                    win,
                )
            }
        },
    }
}

#[cfg(target_os = "windows")]
unsafe fn winit_to_surface<W: SafeBorrow<winit::Window>>(
    instance: Arc<Instance>, win: W,
) -> Result<Arc<Surface<W>>, SurfaceCreationError> {
    use winit::os::windows::WindowExt;
    Surface::from_hwnd(
        instance,
        ptr::null() as *const (), // FIXME
        win.borrow().get_hwnd(),
        win,
    )
}

#[cfg(target_os = "macos")]
unsafe fn winit_to_surface<W: SafeBorrow<winit::Window>>(
    instance: Arc<Instance>, win: W,
) -> Result<Arc<Surface<W>>, SurfaceCreationError> {
    use winit::os::macos::WindowExt;

    let wnd: cocoa_id = mem::transmute(win.borrow().get_nswindow());

    let layer = CoreAnimationLayer::new();

    layer.set_edge_antialiasing_mask(0);
    layer.set_presents_with_transaction(false);
    layer.remove_all_animations();

    let view = wnd.contentView();

    layer.set_contents_scale(view.backingScaleFactor());
    view.setLayer(mem::transmute(layer.as_ref())); // Bombs here with out of memory
    view.setWantsLayer(YES);

    Surface::from_macos_moltenvk(instance, win.borrow().get_nsview() as *const (), win)
}

/// An alternative to `Borrow<T>` with the requirement that all calls to
/// `borrow` return the same object.
pub unsafe trait SafeBorrow<T>: Borrow<T> {}

unsafe impl<T> SafeBorrow<T> for T {}
unsafe impl<'a, T> SafeBorrow<T> for &'a T {}
unsafe impl<'a, T> SafeBorrow<T> for &'a mut T {}
unsafe impl<T> SafeBorrow<T> for Rc<T> {}
unsafe impl<T> SafeBorrow<T> for Arc<T> {}
unsafe impl<T> SafeBorrow<T> for Box<T> {}
