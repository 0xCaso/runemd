//! `application:openURLs:` sul delegate dell'app.
//!
//! winit non implementa il metodo, quindi Launch Services (Finder, "Apri con", `open -a`)
//! non ha a chi consegnare i file. Come fa Rune per il reopen (`platform/reopen.rs`), si
//! aggiunge il metodo alla classe del delegate con `class_addMethod`. Lo si fa alla notifica
//! `NSApplicationWillFinishLaunchingNotification`, quando il delegate di winit esiste e prima
//! che AppKit consegni l'evento di apertura iniziale.

use std::path::PathBuf;
use std::ptr::NonNull;

use block2::RcBlock;
use objc2::ffi::class_addMethod;
use objc2::runtime::{AnyObject, Sel};
use objc2::{MainThreadMarker, msg_send, sel};
use objc2_app_kit::{NSApplication, NSApplicationWillFinishLaunchingNotification};
use objc2_foundation::{NSArray, NSNotification, NSNotificationCenter, NSURL};

extern "C" fn open_urls(_: *mut AnyObject, _: Sel, _: *mut AnyObject, urls: *mut AnyObject) {
    // SAFETY: AppKit passes an NSArray<NSURL> (or nil) valid for the duration of the call.
    let urls: Option<&NSArray<NSURL>> = unsafe { (urls as *const NSArray<NSURL>).as_ref() };
    let paths: Vec<PathBuf> = urls
        .map(|a| a.iter().filter_map(|u| u.absoluteString()).filter_map(|s| super::markdown_path_from_url(&s.to_string())).collect())
        .unwrap_or_default();
    if std::env::var_os("RUNEMD_LOG_OPEN").is_some() {
        eprintln!("runemd: openURLs -> {paths:?}");
    }
    super::queue::push(paths);
}

fn hook(mtm: MainThreadMarker) {
    let app = NSApplication::sharedApplication(mtm);
    let selector = sel!(application:openURLs:);
    // SAFETY: main thread; adding a method the class lacks changes none of its other methods.
    // `v@:@@` is void return, self, _cmd, two objects.
    unsafe {
        let delegate: *mut AnyObject = msg_send![&app, delegate];
        let Some(delegate) = delegate.as_ref() else {
            eprintln!("runemd: nessun delegate, apertura da Finder non attiva");
            return;
        };
        let class = delegate.class();
        let answers: bool = msg_send![delegate, respondsToSelector: selector];
        if answers {
            return;
        }
        type Imp = extern "C" fn(*mut AnyObject, Sel, *mut AnyObject, *mut AnyObject);
        let imp = open_urls as Imp;
        if !class_addMethod(class as *const _ as *mut _, selector, std::mem::transmute::<Imp, unsafe extern "C-unwind" fn()>(imp), c"v@:@@".as_ptr()).as_bool() {
            eprintln!("runemd: impossibile aggiungere application:openURLs: al delegate");
            return;
        }
        // AppKit memorizza quali metodi opzionali il delegate risponde quando lo si imposta.
        let _: () = msg_send![&app, setDelegate: delegate];
    }
}

pub fn install() {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let block = RcBlock::new(move |_: NonNull<NSNotification>| hook(mtm));
    // SAFETY: main thread; the observer lives for the whole process (token leaked on purpose).
    unsafe {
        let center = NSNotificationCenter::defaultCenter();
        let token = center.addObserverForName_object_queue_usingBlock(Some(NSApplicationWillFinishLaunchingNotification), None, None, &block);
        std::mem::forget(token);
    }
}
