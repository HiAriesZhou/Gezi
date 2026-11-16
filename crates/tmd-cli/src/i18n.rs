//! UI language only; never translates document text or writes system preferences.
use std::cell::Cell;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum LanguageChoice {
    Auto,
    En,
    Zh,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    Chinese,
}
thread_local! { static CURRENT: Cell<Language> = const { Cell::new(Language::English) }; }
pub fn current() -> Language {
    CURRENT.get()
}
pub fn set(language: Language) {
    CURRENT.set(language);
}
pub fn toggle() {
    set(if current() == Language::English {
        Language::Chinese
    } else {
        Language::English
    });
}
pub fn text(en: &'static str, zh: &'static str) -> &'static str {
    match current() {
        Language::English => en,
        Language::Chinese => zh,
    }
}

macro_rules! trf {
    ($en:literal, $zh:literal $(, $arg:expr)* $(,)?) => {
        if crate::i18n::current() == crate::i18n::Language::Chinese {
            format!($zh $(, $arg)*)
        } else { format!($en $(, $arg)*) }
    };
}
pub(crate) use trf;

fn parse_tag(value: &str) -> Option<Language> {
    let primary = value
        .trim()
        .split(['-', '_', '.', '@'])
        .next()?
        .to_ascii_lowercase();
    match primary.as_str() {
        "en" | "c" | "posix" => Some(Language::English),
        "zh" => Some(Language::Chinese),
        _ => None,
    }
}

fn choose(
    choice: Option<LanguageChoice>,
    kaku: &[String],
    system: &[String],
    locale: Option<&str>,
) -> Language {
    match choice {
        Some(LanguageChoice::En) => return Language::English,
        Some(LanguageChoice::Zh) => return Language::Chinese,
        _ => {}
    }
    kaku.iter()
        .find_map(|s| parse_tag(s))
        .or_else(|| system.iter().find_map(|s| parse_tag(s)))
        .or_else(|| locale.and_then(parse_tag))
        .unwrap_or(Language::English)
}

pub fn resolve(choice: Option<LanguageChoice>) -> Language {
    if matches!(choice, Some(LanguageChoice::En | LanguageChoice::Zh)) {
        return choose(choice, &[], &[], None);
    }
    let in_kaku = std::env::var("TERM_PROGRAM").as_deref() == Ok("Kaku");
    let (kaku, system) = platform_languages(in_kaku);
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|key| std::env::var(key).ok().filter(|s| !s.is_empty()));
    choose(choice, &kaku, &system, locale.as_deref())
}

#[cfg(target_os = "macos")]
fn platform_languages(in_kaku: bool) -> (Vec<String>, Vec<String>) {
    use core_foundation::{
        array::CFArray,
        base::{CFType, TCFType},
        string::CFString,
    };
    use core_foundation_sys::preferences::*;
    use core_foundation_sys::string::CFStringRef;
    fn read(app: CFStringRef) -> Vec<String> {
        let key = CFString::new("AppleLanguages");
        // Exact domains avoid inheriting global preferences as a Kaku setting.
        // Copy returns an owned object; the safe wrappers release it on drop.
        for host in unsafe { [kCFPreferencesCurrentHost, kCFPreferencesAnyHost] } {
            let value = unsafe {
                CFPreferencesCopyValue(
                    key.as_concrete_TypeRef(),
                    app,
                    kCFPreferencesCurrentUser,
                    host,
                )
            };
            if value.is_null() {
                continue;
            }
            let value = unsafe { CFType::wrap_under_create_rule(value) };
            if let Some(array) = value.downcast::<CFArray>() {
                let languages: Vec<_> = array
                    .iter()
                    .take(32)
                    .filter_map(|v| {
                        if (*v).is_null() {
                            return None;
                        }
                        // Array elements are borrowed CF objects; retain before wrapping.
                        unsafe { CFType::wrap_under_get_rule(*v) }.downcast::<CFString>()
                    })
                    .map(|s| s.to_string())
                    .filter(|s| s.len() <= 128)
                    .collect();
                if !languages.is_empty() {
                    return languages;
                }
            }
        }
        Vec::new()
    }
    let kaku = if in_kaku {
        read(CFString::new("fun.tw93.kaku").as_concrete_TypeRef())
    } else {
        Vec::new()
    };
    let system = read(unsafe { kCFPreferencesAnyApplication });
    (kaku, system)
}
#[cfg(not(target_os = "macos"))]
fn platform_languages(_: bool) -> (Vec<String>, Vec<String>) {
    (Vec::new(), Vec::new())
}

#[cfg(test)]
pub struct Guard(Language);
#[cfg(test)]
impl Drop for Guard {
    fn drop(&mut self) {
        set(self.0);
    }
}
#[cfg(test)]
pub fn scoped(language: Language) -> Guard {
    let old = current();
    set(language);
    Guard(old)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tags(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| (*s).into()).collect()
    }
    #[test]
    fn precedence_and_unsupported_languages_are_explicit() {
        let zh = tags(&["zh-Hans-CN"]);
        let en = tags(&["en-GB"]);
        assert_eq!(
            choose(None, &zh, &en, Some("en_US.UTF-8")),
            Language::Chinese
        );
        assert_eq!(
            choose(None, &[], &zh, Some("en_US.UTF-8")),
            Language::Chinese
        );
        assert_eq!(
            choose(Some(LanguageChoice::En), &zh, &zh, None),
            Language::English
        );
        assert_eq!(
            choose(Some(LanguageChoice::Zh), &en, &en, None),
            Language::Chinese
        );
        assert_eq!(
            choose(
                Some(LanguageChoice::Auto),
                &tags(&["ja", "zh_TW"]),
                &en,
                None
            ),
            Language::Chinese
        );
        assert_eq!(
            choose(None, &tags(&["ja"]), &en, Some("zh_CN")),
            Language::English
        );
        assert_eq!(
            choose(None, &[], &[], Some("zh_CN.UTF-8")),
            Language::Chinese
        );
        for tag in [
            None,
            Some("de_DE.UTF-8"),
            Some(""),
            Some("C"),
            Some("POSIX"),
        ] {
            assert_eq!(choose(None, &[], &[], tag), Language::English);
        }
        assert_eq!(parse_tag("english"), None);
    }
    #[test]
    fn switching_is_thread_local_and_reversible() {
        let _guard = scoped(Language::English);
        assert_eq!(text("Copy", "复制"), "Copy");
        toggle();
        assert_eq!(text("Copy", "复制"), "复制");
        assert_eq!(
            std::thread::spawn(current).join().unwrap(),
            Language::English
        );
        toggle();
        assert_eq!(current(), Language::English);
    }
}
