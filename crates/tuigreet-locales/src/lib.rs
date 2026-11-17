use std::{ops::Deref, sync::OnceLock};

use i18n_embed::{
  DesktopLanguageRequester,
  LanguageLoader,
  fluent::{FluentLanguageLoader, fluent_language_loader},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "locales"]
struct Localizations;

pub struct LazyLoader {
  once: OnceLock<FluentLanguageLoader>,
}

impl LazyLoader {
  const fn new() -> Self {
    Self {
      once: OnceLock::new(),
    }
  }
}

impl Deref for LazyLoader {
  type Target = FluentLanguageLoader;

  #[expect(
    clippy::expect_used,
    reason = "the fallback locale is embedded at build time; invalid bundled \
              translations are a packaging error"
  )]
  fn deref(&self) -> &Self::Target {
    self.once.get_or_init(|| {
      let locales = Localizations;
      let loader = fluent_language_loader!();
      loader
        .load_languages(&locales, &[loader.fallback_language().clone()])
        .expect("embedded fallback translations must be valid");

      let _ = i18n_embed::select(
        &loader,
        &locales,
        &DesktopLanguageRequester::requested_languages(),
      );

      loader
    })
  }
}

pub static MESSAGES: LazyLoader = LazyLoader::new();

#[cfg(test)]
mod tests {
  use std::collections::HashMap;

  use super::MESSAGES;

  #[test]
  fn embedded_catalog_formats_the_authentication_title() {
    let hostname = "release-fixture.example";
    let title = MESSAGES.get_args(
      "title_authenticate",
      HashMap::from([("hostname", hostname)]),
    );
    assert!(title.contains(hostname));
  }
}
