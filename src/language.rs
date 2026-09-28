// SPDX-License-Identifier: MPL-2.0

//! Follows the user's system language through AccountsService, which is where
//! COSMIC Settings writes it (`Language` / `Languages`, both `emits-change`). `Language` is the source of truth.

use cosmic::iced::futures::{stream, SinkExt, Stream, StreamExt};
use zbus::{proxy, Connection};

#[proxy(
    interface = "org.freedesktop.Accounts.User",
    default_service = "org.freedesktop.Accounts"
)]
trait AccountsUser {
    #[zbus(property)]
    fn language(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn languages(&self) -> zbus::Result<Vec<String>>;
}

/// Preference list: `Language` first (COSMIC Settings only sets it, so it wins over a stale
/// `Languages`), then the other non-empty `Languages` entries, without duplicates.
#[must_use]
pub fn pick(languages: Vec<String>, language: String) -> Vec<String> {
    let language = language.trim().to_string();
    let mut list = Vec::new();
    if !language.is_empty() {
        list.push(language.clone());
    }
    list.extend(languages.into_iter().filter(|s| !s.trim().is_empty() && *s != language));
    list
}

fn user_path() -> String {
    use std::os::unix::fs::MetadataExt;
    // Owner of /proc/self is the current uid.
    let uid = std::fs::metadata("/proc/self").map(|m| m.uid()).unwrap_or(1000);
    format!("/org/freedesktop/Accounts/User{uid}")
}

async fn user_proxy(conn: &Connection) -> zbus::Result<AccountsUserProxy<'_>> {
    AccountsUserProxy::builder(conn).path(user_path())?.build().await
}

async fn read(p: &AccountsUserProxy<'_>) -> Vec<String> {
    pick(p.languages().await.unwrap_or_default(), p.language().await.unwrap_or_default())
}

/// Current preference list; empty if AccountsService is unreachable.
pub async fn current() -> Vec<String> {
    let Ok(conn) = Connection::system().await else { return vec![] };
    match user_proxy(&conn).await {
        Ok(p) => read(&p).await,
        Err(_) => vec![],
    }
}

/// Yields the new preference list every time the user's language changes.
/// Never ends and never errors: if D-Bus is unavailable it simply stays silent.
pub fn changes() -> impl Stream<Item = Vec<String>> {
    cosmic::iced::stream::channel(4, |mut out: cosmic::iced::futures::channel::mpsc::Sender<Vec<String>>| async move {
        let Ok(conn) = Connection::system().await else { return std::future::pending().await };
        let Ok(p) = user_proxy(&conn).await else { return std::future::pending().await };
        let mut events = stream::select(
            p.receive_languages_changed().await.map(|_| ()),
            p.receive_language_changed().await.map(|_| ()),
        );
        while events.next().await.is_some() {
            let _ = out.send(read(&p).await).await;
        }
        std::future::pending().await
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn language_wins_over_a_stale_languages_list() {
        assert_eq!(pick(v(&["pt_BR"]), "en_US".into()), v(&["en_US", "pt_BR"]));
    }

    #[test]
    fn language_first_then_other_languages_without_duplicates() {
        assert_eq!(pick(v(&["pt_BR", "en_US"]), "pt_BR".into()), v(&["pt_BR", "en_US"]));
    }

    #[test]
    fn falls_back_to_languages_when_language_is_empty() {
        assert_eq!(pick(v(&["pt_BR", " "]), String::new()), v(&["pt_BR"]));
    }

    #[test]
    fn empty_when_nothing_is_set() {
        assert!(pick(vec![], String::new()).is_empty());
    }
}
