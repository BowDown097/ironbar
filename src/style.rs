use crate::send;
use color_eyre::{Help, Report};
use glib::Continue;
use gtk::ffi::GTK_STYLE_PROVIDER_PRIORITY_USER;
use gtk::prelude::CssProviderExt;
use gtk::{gdk, gio, CssProvider, StyleContext};
use notify::event::ModifyKind;
use notify::{recommended_watcher, Event, EventKind, RecursiveMode, Result, Watcher};
use std::path::PathBuf;
use std::time::Duration;
use tokio::spawn;
use tokio::time::sleep;
use tracing::{debug, error, info};

/// Attempts to load CSS file at the given path
/// and attach if to the current GTK application.
///
/// Installs a file watcher and reloads CSS when
/// write changes are detected on the file.
pub fn load_css(style_path: PathBuf) {
    let provider = CssProvider::new();

    match provider.load_from_file(&gio::File::for_path(&style_path)) {
        Ok(()) => debug!("Loaded css from '{}'", style_path.display()),
        Err(err) => error!("{:?}", Report::new(err)
                    .wrap_err("Failed to load CSS")
                    .suggestion("Check the CSS file for errors")
                    .suggestion("GTK CSS uses a subset of the full CSS spec and many properties are not available. Ensure you are not using any unsupported property.")
                )
    };

    let screen = gdk::Screen::default().expect("Failed to get default GTK screen");
    StyleContext::add_provider_for_screen(
        &screen,
        &provider,
        GTK_STYLE_PROVIDER_PRIORITY_USER as u32,
    );

    let (tx, rx) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);

    spawn(async move {
        let style_path2 = style_path.clone();
        let mut watcher = recommended_watcher(move |res: Result<Event>| match res {
            Ok(event) if matches!(event.kind, EventKind::Modify(ModifyKind::Data(_))) => {
                debug!("{event:?}");
                if event
                    .paths
                    .first()
                    .map(|p| p == &style_path2)
                    .unwrap_or_default()
                {
                    send!(tx, style_path2.clone());
                }
            }
            Err(e) => error!("Error occurred when watching stylesheet: {:?}", e),
            _ => {}
        })
        .expect("Failed to create CSS file watcher");

        let dir_path = style_path.parent().expect("to exist");

        watcher
            .watch(dir_path, RecursiveMode::NonRecursive)
            .expect("Failed to start CSS file watcher");
        debug!("Installed CSS file watcher on '{}'", style_path.display());

        // avoid watcher from dropping
        loop {
            sleep(Duration::from_secs(1)).await;
        }
    });

    {
        rx.attach(None, move |path| {
            info!("Reloading CSS");
            if let Err(err) = provider
                .load_from_file(&gio::File::for_path(path)) {
                error!("{:?}", Report::new(err)
                    .wrap_err("Failed to load CSS")
                    .suggestion("Check the CSS file for errors")
                    .suggestion("GTK CSS uses a subset of the full CSS spec and many properties are not available. Ensure you are not using any unsupported property.")
                );
            }

            Continue(true)
        });
    }
}
