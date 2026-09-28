// SPDX-License-Identifier: MPL-2.0

mod app;
mod config;
mod i18n;
mod language;
mod idle;
mod nm;
mod ui;

fn main() -> cosmic::iced::Result {
    // First frame uses the environment language; AccountsService refines it right after start.
    i18n::select(&i18n_embed::DesktopLanguageRequester::requested_languages());

    // Starts the applet's event loop with `()` as the application's flags.
    cosmic::applet::run::<app::AppModel>(())
}
