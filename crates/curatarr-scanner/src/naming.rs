//! File and folder naming templates.
//!
//! A template is a path-like string containing `{Token}` placeholders, e.g.
//! `{Author}/{Series}/{SeriesPositionPadded} - {Title}.{Extension}`. Token values
//! are substituted from a [`NamingContext`], sanitised for the filesystem, and the
//! resulting path is trimmed of artefacts left behind by empty tokens.

use curatarr_core::error::ScannerError;
use std::path::{Path, PathBuf};

/// Characters that are never allowed inside a path component.
const ILLEGAL_CHARS: &[char] = &['\0', '/', '\\', ':', '*', '?', '"', '<', '>', '|'];

/// Values available for substitution into a naming template.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NamingContext {
    pub title: Option<String>,
    /// Display name, e.g. "Frank Herbert".
    pub author: Option<String>,
    /// Sort name, e.g. "Herbert, Frank". Derived from `author` via
    /// [`sort_name_for`] when absent.
    pub author_sort: Option<String>,
    pub series: Option<String>,
    /// `1.0` renders as `1` / `01`; `1.5` renders as `1.5` / `01.5`.
    pub series_position: Option<f64>,
    pub year: Option<i32>,
    pub publisher: Option<String>,
    pub language: Option<String>,
    pub isbn: Option<String>,
    /// File extension without a leading dot.
    pub extension: String,
}

/// Sanitisation and length limits applied when rendering a template.
#[derive(Debug, Clone, PartialEq)]
pub struct NamingOptions {
    /// Character substituted for illegal characters inside token values.
    pub replacement: char,
    /// Maximum size in bytes of any single path component.
    pub max_component_bytes: usize,
    /// Maximum size in bytes of the whole rendered relative path.
    pub max_path_bytes: usize,
}

impl Default for NamingOptions {
    fn default() -> Self {
        Self {
            replacement: '_',
            max_component_bytes: 255,
            max_path_bytes: 4096,
        }
    }
}

/// The result of previewing a rename against a template.
#[derive(Debug, Clone, PartialEq)]
pub struct RenamePlan {
    pub from: PathBuf,
    pub to: PathBuf,
    pub changed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    Title,
    Author,
    AuthorSort,
    Series,
    SeriesPosition,
    SeriesPositionPadded,
    Year,
    Publisher,
    Language,
    Isbn,
    Extension,
}

impl Token {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "Title" => Some(Self::Title),
            "Author" => Some(Self::Author),
            "AuthorSort" => Some(Self::AuthorSort),
            "Series" => Some(Self::Series),
            "SeriesPosition" => Some(Self::SeriesPosition),
            "SeriesPositionPadded" => Some(Self::SeriesPositionPadded),
            "Year" => Some(Self::Year),
            "Publisher" => Some(Self::Publisher),
            "Language" => Some(Self::Language),
            "Isbn" => Some(Self::Isbn),
            "Extension" | "Format" => Some(Self::Extension),
            _ => None,
        }
    }

    fn fallback(self) -> &'static str {
        match self {
            Self::Title => "Unknown Title",
            Self::Author | Self::AuthorSort => "Unknown Author",
            _ => "",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Part {
    Literal(String),
    Token(Token),
}

/// Render `template` with `ctx` using [`NamingOptions::default`].
pub fn apply_template(template: &str, ctx: &NamingContext) -> Result<PathBuf, ScannerError> {
    apply_template_with(template, ctx, &NamingOptions::default())
}

/// Render `template` with `ctx` using explicit options.
///
/// Returns a relative path. Fails on an empty template, an unknown token, an
/// unbalanced brace, or a template that renders to nothing at all.
pub fn apply_template_with(
    template: &str,
    ctx: &NamingContext,
    opts: &NamingOptions,
) -> Result<PathBuf, ScannerError> {
    let parts = parse_template(template)?;

    let mut rendered = String::new();
    for part in &parts {
        match part {
            Part::Literal(text) => rendered.push_str(text),
            Part::Token(token) => rendered.push_str(&token_value(*token, ctx, opts)),
        }
    }

    let segments: Vec<String> = rendered
        .split('/')
        .map(clean_segment)
        .filter(|s| !s.is_empty())
        .collect();

    if segments.is_empty() {
        return Err(invalid(template, "template rendered to an empty path"));
    }

    let extension = clean_segment(&sanitize(&ctx.extension, opts));
    let segments = enforce_lengths(segments, &extension, opts);

    Ok(segments.iter().collect())
}

/// Preview where `current` would land under `library_root` if renamed by `template`.
pub fn preview_rename(
    template: &str,
    ctx: &NamingContext,
    current: &Path,
    library_root: &Path,
) -> Result<RenamePlan, ScannerError> {
    let to = library_root.join(apply_template(template, ctx)?);
    Ok(RenamePlan {
        from: current.to_path_buf(),
        changed: current != to,
        to,
    })
}

/// Derive a sort name from a display name using a last-token rule:
/// `"Frank Herbert"` becomes `"Herbert, Frank"`. Single words and names that
/// already contain a comma are returned unchanged (with whitespace collapsed).
pub fn sort_name_for(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let Some((last, rest)) = words.split_last() else {
        return String::new();
    };
    let joined = words.join(" ");
    if joined.contains(',') || rest.is_empty() {
        return joined;
    }
    format!("{last}, {}", rest.join(" "))
}

fn invalid(template: &str, reason: impl Into<String>) -> ScannerError {
    ScannerError::InvalidTemplate {
        template: template.to_string(),
        reason: reason.into(),
    }
}

fn parse_template(template: &str) -> Result<Vec<Part>, ScannerError> {
    if template.trim().is_empty() {
        return Err(invalid(template, "template is empty"));
    }

    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut rest = template;

    while let Some(idx) = rest.find(['{', '}']) {
        literal.push_str(&rest[..idx]);
        let after_brace = &rest[idx + 1..];

        if rest[idx..].starts_with('}') {
            return Err(invalid(template, "unbalanced '}'"));
        }

        let Some(end) = after_brace.find('}') else {
            return Err(invalid(template, "unbalanced '{'"));
        };
        let name = &after_brace[..end];
        if name.contains('{') {
            return Err(invalid(template, "unbalanced '{'"));
        }

        let token = Token::parse(name)
            .ok_or_else(|| invalid(template, format!("unknown token '{{{name}}}'")))?;

        if !literal.is_empty() {
            parts.push(Part::Literal(std::mem::take(&mut literal)));
        }
        parts.push(Part::Token(token));
        rest = &after_brace[end + 1..];
    }

    literal.push_str(rest);
    if !literal.is_empty() {
        parts.push(Part::Literal(literal));
    }

    Ok(parts)
}

fn non_blank(value: Option<&String>) -> Option<&str> {
    value.map(String::as_str).filter(|s| !s.trim().is_empty())
}

fn token_value(token: Token, ctx: &NamingContext, opts: &NamingOptions) -> String {
    let raw: Option<String> = match token {
        Token::Title => non_blank(ctx.title.as_ref()).map(str::to_string),
        Token::Author => non_blank(ctx.author.as_ref()).map(str::to_string),
        Token::AuthorSort => non_blank(ctx.author_sort.as_ref())
            .map(str::to_string)
            .or_else(|| non_blank(ctx.author.as_ref()).map(sort_name_for)),
        Token::Series => non_blank(ctx.series.as_ref()).map(str::to_string),
        Token::SeriesPosition => ctx.series_position.map(|p| format_position(p, false)),
        Token::SeriesPositionPadded => ctx.series_position.map(|p| format_position(p, true)),
        Token::Year => ctx.year.map(|y| y.to_string()),
        Token::Publisher => non_blank(ctx.publisher.as_ref()).map(str::to_string),
        Token::Language => non_blank(ctx.language.as_ref()).map(str::to_string),
        Token::Isbn => non_blank(ctx.isbn.as_ref()).map(str::to_string),
        Token::Extension => non_blank(Some(&ctx.extension)).map(str::to_string),
    };

    raw.map(|value| clean_segment(&sanitize(&value, opts)))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| token.fallback().to_string())
}

fn format_position(position: f64, padded: bool) -> String {
    if !position.is_finite() {
        return String::new();
    }

    let text = if position.fract() == 0.0 {
        format!("{position:.0}")
    } else {
        format!("{position}")
    };

    if !padded {
        return text;
    }

    let (int_part, frac_part) = match text.find('.') {
        Some(i) => (&text[..i], &text[i..]),
        None => (text.as_str(), ""),
    };
    let (sign, digits) = int_part
        .strip_prefix('-')
        .map_or(("", int_part), |digits| ("-", digits));

    format!("{sign}{digits:0>2}{frac_part}")
}

/// Replace characters that are illegal in a path component.
fn sanitize(value: &str, opts: &NamingOptions) -> String {
    value
        .chars()
        .map(|c| {
            if ILLEGAL_CHARS.contains(&c) || c.is_control() {
                opts.replacement
            } else {
                c
            }
        })
        .collect()
}

/// Normalise a single path component: collapse whitespace, remove artefacts left
/// by empty tokens (dangling ` - `, empty `()` / `[]`), and trim trailing dots and
/// spaces. Runs until stable.
fn clean_segment(segment: &str) -> String {
    let mut current = collapse_whitespace(segment);

    loop {
        let mut next = current
            .replace("()", "")
            .replace("[]", "")
            .replace("( )", "")
            .replace("[ ]", "")
            .replace(" .", ".")
            .replace("-.", ".");
        next = collapse_whitespace(&next);

        let mut trimmed = next.trim();
        loop {
            let before = trimmed;
            trimmed = trimmed
                .trim_start_matches('-')
                .trim_end_matches('-')
                .trim()
                .trim_end_matches(['.', ' ']);
            if trimmed == before {
                break;
            }
        }

        if trimmed == current {
            return current;
        }
        current = trimmed.to_string();
    }
}

fn collapse_whitespace(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut pending_space = false;
    for c in value.chars() {
        if c.is_whitespace() {
            pending_space = true;
        } else {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.push(c);
        }
    }
    out
}

/// Suffix (including the dot) to keep intact on the final component, if the
/// final component ends with the rendered extension.
fn extension_suffix(last: &str, extension: &str) -> String {
    if extension.is_empty() {
        return String::new();
    }
    let suffix = format!(".{extension}");
    if last.ends_with(&suffix) && last.len() > suffix.len() {
        suffix
    } else {
        String::new()
    }
}

fn enforce_lengths(
    mut segments: Vec<String>,
    extension: &str,
    opts: &NamingOptions,
) -> Vec<String> {
    let count = segments.len();
    let max_component = opts.max_component_bytes.max(1);
    let suffix = segments
        .last()
        .map(|last| extension_suffix(last, extension))
        .unwrap_or_default();

    for (i, segment) in segments.iter_mut().enumerate() {
        if segment.len() > max_component {
            let keep = if i + 1 == count { suffix.as_str() } else { "" };
            *segment = shrink(segment, max_component, keep);
        }
    }

    let mut keep_suffix = !suffix.is_empty();
    loop {
        let total: usize =
            segments.iter().map(String::len).sum::<usize>() + count.saturating_sub(1);
        if total <= opts.max_path_bytes {
            break;
        }
        let excess = total - opts.max_path_bytes;

        let candidate = segments
            .iter()
            .enumerate()
            .map(|(i, segment)| {
                let reserved = if i + 1 == count && keep_suffix && has_suffix(segment, &suffix) {
                    suffix.len()
                } else {
                    0
                };
                let stem = &segment[..segment.len() - reserved];
                let first_char = stem.chars().next().map_or(0, char::len_utf8);
                (i, stem.len().saturating_sub(first_char))
            })
            .max_by_key(|&(_, room)| room);

        match candidate {
            Some((i, room)) if room > 0 => {
                let keep = if i + 1 == count && keep_suffix {
                    suffix.as_str()
                } else {
                    ""
                };
                let target = segments[i].len() - excess.min(room);
                segments[i] = shrink(&segments[i], target, keep);
            }
            _ if keep_suffix => keep_suffix = false,
            _ => break,
        }
    }

    segments
}

fn has_suffix(segment: &str, suffix: &str) -> bool {
    !suffix.is_empty() && segment.ends_with(suffix) && segment.len() > suffix.len()
}

/// Truncate `segment` to at most `target` bytes on a char boundary, keeping
/// `suffix` intact when possible. Never returns an empty string.
fn shrink(segment: &str, target: usize, suffix: &str) -> String {
    let target = target.max(1);
    let (stem, suffix) = if has_suffix(segment, suffix) && target > suffix.len() {
        (&segment[..segment.len() - suffix.len()], suffix)
    } else {
        (segment, "")
    };

    let mut cut = (target - suffix.len()).min(stem.len());
    while cut > 0 && !stem.is_char_boundary(cut) {
        cut -= 1;
    }

    let mut result = stem[..cut].trim_end_matches(['.', ' ']).to_string();
    if result.is_empty() {
        result.push('_');
    }
    result.push_str(suffix);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rstest::rstest;

    const DEFAULT_TEMPLATE: &str = "{Author}/{Series}/{SeriesPositionPadded} - {Title}.{Extension}";

    fn dune() -> NamingContext {
        NamingContext {
            title: Some("Dune".into()),
            author: Some("Frank Herbert".into()),
            author_sort: None,
            series: Some("Dune".into()),
            series_position: Some(1.0),
            year: Some(1965),
            publisher: Some("Chilton Books".into()),
            language: Some("en".into()),
            isbn: Some("9780441013593".into()),
            extension: "epub".into(),
        }
    }

    #[rstest]
    #[case(DEFAULT_TEMPLATE, dune(), "Frank Herbert/Dune/01 - Dune.epub")]
    #[case(
        "{AuthorSort}/{Title} ({Year}).{Extension}",
        dune(),
        "Herbert, Frank/Dune (1965).epub"
    )]
    #[case(
        DEFAULT_TEMPLATE,
        NamingContext { series: None, series_position: None, ..dune() },
        "Frank Herbert/Dune.epub"
    )]
    #[case(
        "{Title} ({Year}).{Extension}",
        NamingContext { year: None, ..dune() },
        "Dune.epub"
    )]
    #[case(
        DEFAULT_TEMPLATE,
        NamingContext { author: None, ..dune() },
        "Unknown Author/Dune/01 - Dune.epub"
    )]
    #[case(
        "{Title}.{Extension}",
        NamingContext { title: Some("Dune: Messiah?".into()), ..dune() },
        "Dune_ Messiah_.epub"
    )]
    #[case(
        "{SeriesPositionPadded}.{Extension}",
        NamingContext { series_position: Some(1.5), ..dune() },
        "01.5.epub"
    )]
    #[case(
        "{SeriesPosition}.{Extension}",
        NamingContext { series_position: Some(1.5), ..dune() },
        "1.5.epub"
    )]
    #[case(
        "{SeriesPositionPadded}.{Extension}",
        NamingContext { series_position: Some(100.0), ..dune() },
        "100.epub"
    )]
    #[case("{Title}.{Format}", dune(), "Dune.epub")]
    #[case(
        "{Title}.{Extension}",
        NamingContext { title: None, ..dune() },
        "Unknown Title.epub"
    )]
    #[case(
        "{Author}/{Title}.{Extension}",
        NamingContext { author: Some("a/b".into()), ..dune() },
        "a_b/Dune.epub"
    )]
    #[case(
        "{Publisher}/{Language}/{Isbn}/{Title}.{Extension}",
        dune(),
        "Chilton Books/en/9780441013593/Dune.epub"
    )]
    #[case(
        "{Author}/{Title}.{Extension}",
        NamingContext { author: Some("Frank   Herbert".into()), ..dune() },
        "Frank Herbert/Dune.epub"
    )]
    fn renders_expected_path(
        #[case] template: &str,
        #[case] ctx: NamingContext,
        #[case] expected: &str,
    ) {
        let result = apply_template(template, &ctx).unwrap();
        assert_eq!(result, PathBuf::from(expected));
    }

    #[rstest]
    #[case("{Author}/{Nope}.{Extension}")]
    #[case("{Author}/{Title.{Extension}")]
    #[case("{Author}/Title}.{Extension}")]
    #[case("{Author}/{{Title}}.{Extension}")]
    #[case("")]
    #[case("   ")]
    fn invalid_templates_error(#[case] template: &str) {
        let err = apply_template(template, &dune()).unwrap_err();
        assert!(
            matches!(err, ScannerError::InvalidTemplate { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn template_rendering_to_nothing_errors() {
        let ctx = NamingContext {
            series: None,
            ..dune()
        };
        let err = apply_template("{Series}", &ctx).unwrap_err();
        assert!(matches!(err, ScannerError::InvalidTemplate { .. }));
    }

    #[test]
    fn explicit_author_sort_wins_over_derived() {
        let ctx = NamingContext {
            author_sort: Some("Le Guin, Ursula K.".into()),
            author: Some("Ursula K. Le Guin".into()),
            ..dune()
        };
        let result = apply_template("{AuthorSort}/{Title}.{Extension}", &ctx).unwrap();
        assert_eq!(result, PathBuf::from("Le Guin, Ursula K/Dune.epub"));
    }

    #[test]
    fn custom_replacement_char_is_used() {
        let opts = NamingOptions {
            replacement: '-',
            ..NamingOptions::default()
        };
        let ctx = NamingContext {
            title: Some("A:B".into()),
            ..dune()
        };
        let result = apply_template_with("{Title}.{Extension}", &ctx, &opts).unwrap();
        assert_eq!(result, PathBuf::from("A-B.epub"));
    }

    #[test]
    fn long_title_truncated_but_extension_kept() {
        let ctx = NamingContext {
            title: Some("x".repeat(400)),
            ..dune()
        };
        let result = apply_template("{Title}.{Extension}", &ctx).unwrap();
        let name = result.to_string_lossy().into_owned();
        assert!(name.len() <= 255);
        assert!(name.ends_with(".epub"));
    }

    #[test]
    fn preview_rename_reports_change() {
        let root = Path::new("/library");
        let plan = preview_rename(
            DEFAULT_TEMPLATE,
            &dune(),
            Path::new("/downloads/dune.epub"),
            root,
        )
        .unwrap();
        assert_eq!(plan.from, PathBuf::from("/downloads/dune.epub"));
        assert_eq!(
            plan.to,
            PathBuf::from("/library/Frank Herbert/Dune/01 - Dune.epub")
        );
        assert!(plan.changed);

        let same = preview_rename(DEFAULT_TEMPLATE, &dune(), &plan.to, root).unwrap();
        assert!(!same.changed);
    }

    #[rstest]
    #[case("Frank Herbert", "Herbert, Frank")]
    #[case("Ursula K. Le Guin", "Guin, Ursula K. Le")]
    #[case("Plato", "Plato")]
    #[case("Herbert, Frank", "Herbert, Frank")]
    #[case("  Frank   Herbert  ", "Herbert, Frank")]
    #[case("", "")]
    #[case("   ", "")]
    fn sort_name_cases(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(sort_name_for(input), expected);
    }

    fn is_illegal(c: char) -> bool {
        ILLEGAL_CHARS.contains(&c) || c.is_control()
    }

    fn arb_text() -> impl Strategy<Value = String> {
        prop::collection::vec(any::<char>(), 0..300).prop_map(String::from_iter)
    }

    fn arb_ctx() -> impl Strategy<Value = NamingContext> {
        (
            prop::option::of(arb_text()),
            prop::option::of(arb_text()),
            prop::option::of(arb_text()),
            prop::option::of(arb_text()),
            prop::option::of(any::<f64>()),
            prop::option::of(any::<i32>()),
            prop::option::of(arb_text()),
            prop::option::of(arb_text()),
            prop::option::of(arb_text()),
            arb_text(),
        )
            .prop_map(
                |(
                    title,
                    author,
                    author_sort,
                    series,
                    series_position,
                    year,
                    publisher,
                    language,
                    isbn,
                    extension,
                )| NamingContext {
                    title,
                    author,
                    author_sort,
                    series,
                    series_position,
                    year,
                    publisher,
                    language,
                    isbn,
                    extension,
                },
            )
    }

    fn components(path: &Path) -> Vec<String> {
        path.components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect()
    }

    proptest! {
        #[test]
        fn output_is_always_filesystem_safe(ctx in arb_ctx()) {
            let path = apply_template(DEFAULT_TEMPLATE, &ctx).unwrap();
            let parts = components(&path);

            prop_assert!(!parts.is_empty());
            for part in &parts {
                prop_assert!(!part.is_empty());
                prop_assert!(part.len() <= 255, "component too long: {}", part.len());
                prop_assert!(!part.chars().any(is_illegal), "illegal char in {part:?}");
                prop_assert!(!part.ends_with(['.', ' ']), "bad trailing char in {part:?}");
            }
            prop_assert!(!parts.last().unwrap().is_empty());
            prop_assert!(path.as_os_str().len() <= 4096);
        }

        #[test]
        fn output_never_exceeds_max_path(ctx in arb_ctx(), max in 40usize..200) {
            let opts = NamingOptions { max_path_bytes: max, ..NamingOptions::default() };
            let path = apply_template_with(DEFAULT_TEMPLATE, &ctx, &opts).unwrap();
            prop_assert!(path.as_os_str().len() <= max, "{} > {max}", path.as_os_str().len());
            let parts = components(&path);
            prop_assert!(!parts.is_empty());
            prop_assert!(parts.iter().all(|p| !p.is_empty()));
        }

        #[test]
        fn small_component_limit_is_respected(ctx in arb_ctx(), max in 1usize..64) {
            let opts = NamingOptions { max_component_bytes: max, ..NamingOptions::default() };
            let path = apply_template_with(DEFAULT_TEMPLATE, &ctx, &opts).unwrap();
            for part in components(&path) {
                prop_assert!(part.len() <= max, "{} > {max}", part.len());
            }
        }

        #[test]
        fn sort_name_never_panics(name in arb_text()) {
            let _ = sort_name_for(&name);
        }
    }
}
