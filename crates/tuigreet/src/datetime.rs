use std::iter::repeat_n;

use jiff::{Zoned, fmt::strtime::BrokenDownTime};
use pure_rust_locales::{Locale, locale_match};

#[derive(Clone, Copy)]
enum LetterCase {
  Preserve,
  Lower,
  Upper,
  Swap,
}

/// Format a zoned datetime with the locale data Chrono previously exposed.
///
/// Jiff handles the strtime grammar and all numeric/time-zone fields. Textual
/// weekday, month, and meridiem directives are written from the embedded GNU
/// locale tables because Jiff deliberately leaves those directives
/// unlocalized.
pub fn format_localized(
  format: &str,
  datetime: &Zoned,
  locale: Locale,
) -> Result<String, jiff::Error> {
  let tm = BrokenDownTime::from(datetime);
  let bytes = format.as_bytes();
  let mut output = String::with_capacity(format.len());
  let mut chunk_start = 0;
  let mut at = 0;

  while at < bytes.len() {
    if bytes[at] != b'%' {
      at += 1;
      continue;
    }

    let mut specifier = at + 1;
    if specifier >= bytes.len() {
      break;
    }
    if bytes[specifier] == b'%' {
      at = specifier + 1;
      continue;
    }

    let flag = match bytes[specifier] {
      flag @ (b'_' | b'-' | b'0' | b'^' | b'#') => {
        specifier += 1;
        Some(flag)
      },
      _ => None,
    };
    let width_start = specifier;
    while specifier < bytes.len() && bytes[specifier].is_ascii_digit() {
      specifier += 1;
    }
    if specifier >= bytes.len() {
      break;
    }

    let Some(value) = localized_value(bytes[specifier], datetime, locale)
    else {
      at = specifier + 1;
      continue;
    };

    tm.format(&format[chunk_start..at], &mut output)?;
    let width = format[width_start..specifier].parse::<usize>().ok();
    let letter_case = match flag {
      Some(b'^') => LetterCase::Upper,
      Some(b'#') => LetterCase::Swap,
      _ if bytes[specifier] == b'P' => LetterCase::Lower,
      _ => LetterCase::Preserve,
    };
    write_localized(&mut output, value, letter_case, flag, width);

    at = specifier + 1;
    chunk_start = at;
  }

  tm.format(&format[chunk_start..], &mut output)?;
  Ok(output)
}

fn localized_value(
  specifier: u8,
  datetime: &Zoned,
  locale: Locale,
) -> Option<&'static str> {
  let weekday = usize::try_from(datetime.weekday().to_sunday_zero_offset())
    .unwrap_or_default();
  let month = usize::try_from(datetime.month() - 1).unwrap_or_default();
  match specifier {
    b'a' => Some(short_weekdays(locale)[weekday]),
    b'A' => Some(long_weekdays(locale)[weekday]),
    b'b' | b'h' => Some(short_months(locale)[month]),
    b'B' => Some(long_months(locale)[month]),
    b'p' | b'P' => Some(meridiem(locale)[usize::from(datetime.hour() >= 12)]),
    _ => None,
  }
}

fn write_localized(
  output: &mut String,
  value: &str,
  letter_case: LetterCase,
  flag: Option<u8>,
  width: Option<usize>,
) {
  if flag != Some(b'-')
    && let Some(width) = width
  {
    let value_width = transformed_len(value, letter_case);
    let padding = width.saturating_sub(value_width);
    let pad = if flag == Some(b'0') { '0' } else { ' ' };
    output.extend(repeat_n(pad, padding));
  }

  for character in value.chars() {
    match letter_case {
      LetterCase::Lower => output.extend(character.to_lowercase()),
      LetterCase::Upper => output.extend(character.to_uppercase()),
      LetterCase::Swap if character.is_lowercase() => {
        output.extend(character.to_uppercase());
      },
      LetterCase::Swap if character.is_uppercase() => {
        output.extend(character.to_lowercase());
      },
      LetterCase::Preserve | LetterCase::Swap => output.push(character),
    }
  }
}

fn transformed_len(value: &str, letter_case: LetterCase) -> usize {
  value
    .chars()
    .map(|character| {
      match letter_case {
        LetterCase::Lower => character.to_lowercase().count(),
        LetterCase::Upper => character.to_uppercase().count(),
        LetterCase::Swap if character.is_lowercase() => {
          character.to_uppercase().count()
        },
        LetterCase::Swap if character.is_uppercase() => {
          character.to_lowercase().count()
        },
        LetterCase::Preserve | LetterCase::Swap => 1,
      }
    })
    .sum()
}

const fn short_weekdays(locale: Locale) -> &'static [&'static str] {
  locale_match!(locale => LC_TIME::ABDAY)
}

const fn long_weekdays(locale: Locale) -> &'static [&'static str] {
  locale_match!(locale => LC_TIME::DAY)
}

const fn short_months(locale: Locale) -> &'static [&'static str] {
  locale_match!(locale => LC_TIME::ABMON)
}

const fn long_months(locale: Locale) -> &'static [&'static str] {
  locale_match!(locale => LC_TIME::MON)
}

const fn meridiem(locale: Locale) -> &'static [&'static str] {
  locale_match!(locale => LC_TIME::AM_PM)
}

#[cfg(test)]
mod tests {
  use std::error::Error;

  use jiff::{civil::date, tz::TimeZone};
  use pure_rust_locales::Locale;

  use super::format_localized;

  const FORMAT: &str = "%a, %d %h %Y - %H:%M";

  #[test]
  fn localizes_german_weekday_and_month() -> Result<(), Box<dyn Error>> {
    let datetime =
      date(2026, 10, 19).at(14, 5, 0, 0).to_zoned(TimeZone::UTC)?;

    assert_eq!(
      format_localized(FORMAT, &datetime, Locale::de_DE)?,
      "Mo, 19 Okt 2026 - 14:05",
    );
    Ok(())
  }

  #[test]
  fn localizes_french_weekday_and_month() -> Result<(), Box<dyn Error>> {
    let datetime =
      date(2026, 10, 19).at(14, 5, 0, 0).to_zoned(TimeZone::UTC)?;

    assert_eq!(
      format_localized(FORMAT, &datetime, Locale::fr_FR)?,
      "lun., 19 oct. 2026 - 14:05",
    );
    Ok(())
  }
  #[test]
  fn preserves_english_names_and_numeric_fields() -> Result<(), Box<dyn Error>>
  {
    let datetime =
      date(2026, 10, 19).at(14, 5, 6, 0).to_zoned(TimeZone::UTC)?;

    assert_eq!(
      format_localized(
        "%a %A %b %B %h %p %P | %F %T",
        &datetime,
        Locale::en_US,
      )?,
      "Mon Monday Oct October Oct PM pm | 2026-10-19 14:05:06",
    );
    assert_eq!(
      format_localized("%F %T", &datetime, Locale::fr_FR)?,
      "2026-10-19 14:05:06",
    );
    Ok(())
  }

  #[test]
  fn preserves_escapes_and_case_flags() -> Result<(), Box<dyn Error>> {
    let datetime =
      date(2026, 10, 19).at(14, 5, 0, 0).to_zoned(TimeZone::UTC)?;

    assert_eq!(
      format_localized("%%a %^a %#b %P", &datetime, Locale::de_DE)?,
      "%a MO oKT ",
    );
    Ok(())
  }
}
