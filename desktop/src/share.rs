//! A file shared with the sheet of the system, as on Android: on macOS its
//! sharing picker (Mail, Messages, AirDrop, the apps that take files), on
//! Windows its Share window. Linux has none: there the file is only saved
//! where the user chooses.

use {iced::Task, std::path::PathBuf};

/// Whether the system has a sheet to share with.
pub const NATIVE: bool = cfg!(any(target_os = "macos", windows));

/// The file shared from the window of the app (the first one opened):
/// what went wrong, if anything.
pub fn share(path: PathBuf, title: String) -> Task<Result<(), String>> {
    iced::window::oldest().then(move |id| match id {
        Some(id) => {
            let path = path.clone();
            let title = title.clone();
            iced::window::run(id, move |window| {
                let handle = window.window_handle().map_err(|e| e.to_string())?.as_raw();
                platform::share(handle, &path, &title)
            })
        }
        None => Task::done(Err("no window".to_string())),
    })
}

/// Where a file to share is written first, under its name: the sheet
/// takes a file. In the temporary folder of the system, one per name.
pub fn staged(name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("scout4all-share");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(name);
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(target_os = "macos")]
mod platform {
    use {
        objc2::{AnyThread, MainThreadMarker, rc::Retained, runtime::AnyObject},
        objc2_app_kit::{NSSharingServicePicker, NSView},
        objc2_foundation::{NSArray, NSPoint, NSRect, NSRectEdge, NSSize, NSString, NSURL},
        raw_window_handle::RawWindowHandle,
        std::{cell::RefCell, path::Path},
    };

    thread_local! {
        /// the picker shown, kept while it is
        static SHOWN: RefCell<Option<Retained<NSSharingServicePicker>>> =
            const { RefCell::new(None) };
    }

    pub fn share(window: RawWindowHandle, path: &Path, _title: &str) -> Result<(), String> {
        let RawWindowHandle::AppKit(handle) = window else {
            return Err("not an AppKit window".to_string());
        };
        // the window runs its callbacks on the main thread
        MainThreadMarker::new().ok_or("not on the main thread")?;
        // SAFETY: the handle is the NSView of the window, alive while the
        // window is, and this runs on the main thread
        let view: &NSView = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
        let item: Retained<AnyObject> = url.into();
        let items = NSArray::from_retained_slice(&[item]);
        // SAFETY: the items are objects the services take (a file URL)
        let picker = unsafe {
            NSSharingServicePicker::initWithItems(NSSharingServicePicker::alloc(), &items)
        };
        // in the middle of the window, a little above
        let bounds = view.bounds();
        let at = NSRect::new(
            NSPoint::new(bounds.size.width / 2.0, bounds.size.height / 2.0),
            NSSize::new(1.0, 1.0),
        );
        picker.showRelativeToRect_ofView_preferredEdge(at, view, NSRectEdge::MinY);
        SHOWN.with(|shown| *shown.borrow_mut() = Some(picker));
        Ok(())
    }
}

#[cfg(windows)]
mod platform {
    use {
        raw_window_handle::RawWindowHandle,
        std::{cell::RefCell, path::Path},
        windows::{
            ApplicationModel::DataTransfer::{DataRequestedEventArgs, DataTransferManager},
            Foundation::TypedEventHandler,
            Storage::{IStorageItem, StorageFile},
            Win32::{Foundation::HWND, UI::Shell::IDataTransferManagerInterop},
            core::{HSTRING, Interface, Ref, factory},
        },
        windows_collections::IIterable,
    };

    thread_local! {
        /// the handler of the last share, removed before the next one (on
        /// the thread of the window)
        static HANDLER: RefCell<Option<(DataTransferManager, i64)>> =
            const { RefCell::new(None) };
    }

    pub fn share(window: RawWindowHandle, path: &Path, title: &str) -> Result<(), String> {
        let RawWindowHandle::Win32(handle) = window else {
            return Err("not a Win32 window".to_string());
        };
        let hwnd = HWND(handle.hwnd.get() as *mut core::ffi::c_void);
        let error = |e: windows::core::Error| e.message();
        let interop: IDataTransferManagerInterop =
            factory::<DataTransferManager, IDataTransferManagerInterop>().map_err(error)?;
        // SAFETY: the window is the app's, alive while this runs
        let manager: DataTransferManager = unsafe { interop.GetForWindow(hwnd) }.map_err(error)?;
        let path = HSTRING::from(path.as_os_str());
        let title = HSTRING::from(title);
        let handler = TypedEventHandler::new(
            move |_: Ref<DataTransferManager>, args: Ref<DataRequestedEventArgs>| {
                let Some(args) = args.as_ref() else {
                    return Ok(());
                };
                let data = args.Request()?.Data()?;
                data.Properties()?.SetTitle(&title)?;
                let file = StorageFile::GetFileFromPathAsync(&path)?.join()?;
                let items: IIterable<IStorageItem> =
                    vec![Some(file.cast::<IStorageItem>()?)].into();
                data.SetStorageItemsReadOnly(&items)
            },
        );
        let token = manager.DataRequested(&handler).map_err(error)?;
        if let Some((manager, token)) =
            HANDLER.with(|last| last.borrow_mut().replace((manager, token)))
        {
            let _ = manager.RemoveDataRequested(token);
        }
        // SAFETY: as above
        unsafe { interop.ShowShareUIForWindow(hwnd) }.map_err(error)
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod platform {
    use {raw_window_handle::RawWindowHandle, std::path::Path};

    pub fn share(_: RawWindowHandle, _: &Path, _: &str) -> Result<(), String> {
        Err("no share sheet on this system".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The file to share written under its name, again with the latest
    /// bytes.
    #[test]
    fn the_file_staged_to_share() {
        let path = staged("scout4all-test.zip", b"one").unwrap();
        assert_eq!(path.file_name().unwrap(), "scout4all-test.zip");
        assert_eq!(std::fs::read(&path).unwrap(), b"one");
        let again = staged("scout4all-test.zip", b"two").unwrap();
        assert_eq!(std::fs::read(&again).unwrap(), b"two");
        let _ = std::fs::remove_file(path);
    }
}
