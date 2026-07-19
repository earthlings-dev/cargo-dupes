//! Generic text-grain detection: tokenization with per-language quote
//! profiles, sliding token windows (raw and normalized), structural line
//! windows with stanza coalescing, and their suppression/admission tagging.

use std::path::Path;

use crate::code_unit::CodeUnit;
use crate::code_unit::CodeUnitKind;
use crate::code_unit::DetectionDimension;
use crate::config::Config;
use crate::fingerprint::Fingerprint;
use crate::node::NodeKind;
use crate::node::NormalizedNode;
use crate::suppression::RuleId;
use crate::suppression::SuppressionPolicy;

/// Generic token and line units extracted from a text-like file.
#[derive(Debug, Default)]
pub struct TextUnits {
  /// Identifier/literal-normalized token windows.
  pub normalized_tokens: Vec<CodeUnit>,
  /// Raw token windows.
  pub raw_tokens:        Vec<CodeUnit>,
  /// Normalized line windows.
  pub lines:             Vec<CodeUnit>,
}

/// Extract generic token and line units from a source file.
#[must_use]
pub fn extract(path: &Path, source: &str, config: &Config) -> TextUnits {
  let tokens = tokenize(source, QuoteProfile::for_path(path));
  let normalized_tokens = token_windows_if_enabled(config, path, &tokens, TokenMode::Normalized);
  let raw_tokens = token_windows_if_enabled(config, path, &tokens, TokenMode::Raw);
  let lines = if config.dimension_enabled(DetectionDimension::Line) {
    line_windows(path, source, config.line_min_lines, &config.suppression)
  } else {
    Vec::new()
  };
  TextUnits {
    normalized_tokens,
    raw_tokens,
    lines,
  }
}

/// Token representation with source line information.
#[derive(Debug, Clone)]
struct Token {
  /// Whitespace-insensitive source token.
  raw:        String,
  /// Identifier/literal-normalized token.
  normalized: String,
  /// One-based source line.
  line:       usize,
  /// One-based line of the token's last character (differs from `line`
  /// only for multi-line string tokens).
  end_line:   usize,
}

impl Token {
  /// True when `next` begins on the same or the immediately following
  /// source line, so no token-free line separates the two tokens.
  const fn adjoins(&self, next: &Self) -> bool {
    next.line <= self.end_line + 1
  }
}

/// Token window extraction mode.
#[derive(Debug, Clone, Copy)]
enum TokenMode {
  /// Use normalized tokens.
  Normalized,
  /// Use raw tokens.
  Raw,
}

/// Quote lexing profile, selected by file extension.
///
/// The default pairs `"`, `'`, and `` ` `` naively. Rust sources lex `'` as
/// a quoted token only for char-literal shapes that close on the same line
/// (`'X'`, `'\n'`, `'\u{10FFFF}'`); any other tick — lifetimes, loop labels,
/// prose apostrophes in comments — is punctuation. Without this, one
/// unpaired apostrophe opens a phantom multi-line "string" that runs to the
/// next apostrophe anywhere in the file, bridging blank lines and silently
/// swallowing whole spans out of token segmentation.
#[derive(Debug, Clone, Copy, Default)]
struct QuoteProfile {
  /// Lex `'` per Rust char-literal rules instead of naive pairing.
  rust_ticks: bool,
}

impl QuoteProfile {
  fn for_path(path: &Path) -> Self {
    Self {
      rust_ticks: path.extension().is_some_and(|ext| ext == "rs"),
    }
  }
}

/// Length in chars of a char-literal tail following an opening `'`
/// (the body plus closing quote), or `None` when the tick does not open a
/// char literal. Literals close on the same line within a small bound:
/// `'X'`, `'\n'`, `'\''`, `'\u{10FFFF}'`.
fn char_literal_tail(rest: &str) -> Option<usize> {
  const MAX_TAIL: usize = 11; // \u{10FFFF} plus the closing quote
  let mut tail = rest.chars();
  let first = tail.next()?;
  if first == '\\' {
    let escaped = tail.next()?;
    if escaped == '\n' {
      return None;
    }
    let mut len = 2;
    for ch in tail {
      len += 1;
      if len > MAX_TAIL || ch == '\n' {
        return None;
      }
      if ch == '\'' {
        return Some(len);
      }
    }
    None
  } else if first != '\'' && first != '\n' && tail.next() == Some('\'') {
    Some(2)
  } else {
    None
  }
}

/// Convert source into coarse language-agnostic tokens.
fn tokenize(source: &str, profile: QuoteProfile) -> Vec<Token> {
  let mut tokens = Vec::new();
  let mut chars = source.char_indices().peekable();
  let mut line = 1_usize;

  while let Some((idx, ch)) = chars.next() {
    if ch == '\n' {
      line += 1;
      continue;
    }
    if ch.is_whitespace() {
      continue;
    }

    if is_ident_start(ch) {
      let mut raw = String::from(ch);
      consume_peeked_while(&mut chars, &mut raw, is_ident_continue);
      let normalized = if is_keyword(&raw) {
        raw.clone()
      } else {
        "IDENT".to_string()
      };
      tokens.push(Token {
        raw,
        normalized,
        line,
        end_line: line,
      });
      continue;
    }

    if ch.is_ascii_digit() {
      let mut raw = String::from(ch);
      consume_peeked_while(&mut chars, &mut raw, |next| {
        next.is_ascii_alphanumeric() || matches!(next, '_' | '.')
      });
      tokens.push(Token {
        raw,
        normalized: "NUMBER".to_string(),
        line,
        end_line: line,
      });
      continue;
    }

    let naive_quote = match ch {
      '"' | '`' => true,
      '\'' => !profile.rust_ticks,
      _ => false,
    };
    if naive_quote {
      let quote = ch;
      let start_line = line;
      let mut raw = String::from(ch);
      let mut escaped = false;
      for (_, next) in chars.by_ref() {
        raw.push(next);
        if next == '\n' {
          line += 1;
        }
        if escaped {
          escaped = false;
        } else if next == '\\' {
          escaped = true;
        } else if next == quote {
          break;
        }
      }
      tokens.push(Token {
        raw,
        normalized: "STRING".to_string(),
        line: start_line,
        end_line: line,
      });
      continue;
    }

    // Rust tick: char literals pair like strings; any other tick
    // (lifetime, loop label, prose apostrophe in a comment) falls
    // through to the punctuation arm below.
    if ch == '\''
      && let Some(tail_len) = char_literal_tail(&source[idx + 1..])
    {
      let mut raw = String::from(ch);
      for (_, next) in chars.by_ref().take(tail_len) {
        raw.push(next);
      }
      tokens.push(Token {
        raw,
        normalized: "STRING".to_string(),
        line,
        end_line: line,
      });
      continue;
    }

    let raw = String::from(ch);
    tokens.push(Token {
      raw: raw.clone(),
      normalized: raw,
      line,
      end_line: line,
    });
  }

  tokens
}

/// Append peeked characters to `raw` while `keep` accepts them.
fn consume_peeked_while(chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>, raw: &mut String, mut keep: impl FnMut(char) -> bool) {
  while let Some((_, next)) = chars.peek().copied() {
    if !keep(next) {
      break;
    }
    raw.push(next);
    let _ = chars.next();
  }
}

/// Return true if `ch` can start an identifier-like token.
const fn is_ident_start(ch: char) -> bool {
  ch == '_' || ch.is_ascii_alphabetic()
}

/// Return true if `ch` can continue an identifier-like token.
const fn is_ident_continue(ch: char) -> bool {
  ch == '_' || ch == '-' || ch.is_ascii_alphanumeric()
}

/// Keep common language keywords structurally meaningful in normalized tokens.
fn is_keyword(token: &str) -> bool {
  matches!(
    token,
    "as"
      | "async"
      | "await"
      | "break"
      | "class"
      | "const"
      | "continue"
      | "def"
      | "else"
      | "enum"
      | "false"
      | "fn"
      | "for"
      | "from"
      | "if"
      | "impl"
      | "import"
      | "in"
      | "let"
      | "loop"
      | "match"
      | "mod"
      | "mut"
      | "pub"
      | "return"
      | "self"
      | "static"
      | "struct"
      | "trait"
      | "true"
      | "type"
      | "use"
      | "where"
      | "while"
      | "yield"
  )
}

/// Build token windows for one mode, or nothing when its dimension is off.
fn token_windows_if_enabled(config: &Config, path: &Path, tokens: &[Token], mode: TokenMode) -> Vec<CodeUnit> {
  let dimension = match mode {
    TokenMode::Normalized => DetectionDimension::TokenNormalized,
    TokenMode::Raw => DetectionDimension::TokenRaw,
  };
  if config.dimension_enabled(dimension) {
    token_windows(
      path, tokens, config.token_min_tokens, config.token_min_lines, mode, &config.suppression,
    )
  } else {
    Vec::new()
  }
}

/// Build token windows.
///
/// One window is anchored at the start of each segment (a contiguous run of
/// token-bearing source lines; blank lines separate segments unless bridged
/// by a multi-line string token). Anchoring to concept starts keeps token
/// matching stable when unrelated code earlier in the file shifts, and makes
/// each window represent a complete test case, function, or data stanza
/// rather than an arbitrary mid-concept boilerplate slice.
fn token_windows(
  path: &Path,
  tokens: &[Token],
  min_tokens: usize,
  min_lines: usize,
  mode: TokenMode,
  policy: &SuppressionPolicy,
) -> Vec<CodeUnit> {
  if min_tokens == 0 || tokens.len() < min_tokens {
    return Vec::new();
  }
  let mut units = Vec::new();
  for segment in token_segments(tokens) {
    let Some(slice) = anchored_window(segment, min_tokens, min_lines) else {
      continue;
    };
    let line_start = slice.first().map_or(1, |token| token.line);
    let line_end = slice.last().map_or(1, |token| token.end_line);
    let suppressed = classify_token_window(segment, slice, mode, policy);
    let values: Vec<String> = slice
      .iter()
      .map(|token| match mode {
        TokenMode::Normalized => token.normalized.clone(),
        TokenMode::Raw => token.raw.clone(),
      })
      .collect();
    let mut unit = window_unit(
      path,
      match mode {
        TokenMode::Normalized => "normalized token window",
        TokenMode::Raw => "raw token window",
      },
      CodeUnitKind::TokenWindow,
      line_start,
      line_end,
      &values,
    );
    unit.suppressed = suppressed;
    units.push(unit);
  }
  units
}

/// The shortest segment prefix satisfying both token and line minimums.
///
/// The prefix is a deterministic function of the segment content alone, so
/// identical duplicated segments always produce identical windows.
fn anchored_window(segment: &[Token], min_tokens: usize, min_lines: usize) -> Option<&[Token]> {
  if segment.len() < min_tokens {
    return None;
  }
  let line_start = segment.first()?.line;
  let mut end = min_tokens;
  while line_span(line_start, segment[end - 1].end_line) < min_lines {
    if end == segment.len() {
      return None;
    }
    end += 1;
  }
  Some(&segment[..end])
}

/// Split a token stream into segments separated by token-free source lines.
fn token_segments(tokens: &[Token]) -> Vec<&[Token]> {
  crate::runs::split_runs_by(tokens, Token::adjoins)
}

/// Build normalized line windows.
///
/// Windows slide within contiguous content-line segments only; a blank line
/// (or a line emptied by block-comment stripping) is a concept boundary that
/// no window may cross. Windows are also rejected when they end on a
/// block-opening line or start on a closing-delimiter line, so signature or
/// skeleton fragments cut mid-concept never become standalone units.
fn line_windows(path: &Path, source: &str, min_lines: usize, policy: &SuppressionPolicy) -> Vec<CodeUnit> {
  if min_lines == 0 {
    return Vec::new();
  }
  let mut in_block_comment = false;
  let normalized: Vec<(usize, String)> = source
    .lines()
    .enumerate()
    .filter_map(|(idx, line)| {
      let without_block_comments = strip_block_comments(line, &mut in_block_comment);
      let trimmed = without_block_comments.split_whitespace().collect::<Vec<_>>().join(" ");
      if trimmed.is_empty() {
        None
      } else {
        Some((idx + 1, trimmed))
      }
    })
    .collect();
  if normalized.len() < min_lines {
    return Vec::new();
  }
  let mut units = Vec::new();
  for segment in coalesce_stanza_segments(line_segments(&normalized)) {
    let segment = segment.as_slice();
    if segment.len() < min_lines {
      continue;
    }
    for start in 0..=(segment.len() - min_lines) {
      let slice = &segment[start..start + min_lines];
      if !line_window_is_aligned(slice) {
        continue;
      }
      let suppressed = classify_line_window(segment, start, min_lines, policy);
      let values: Vec<String> = slice.iter().map(|(_, line)| line.clone()).collect();
      let mut unit = window_unit(
        path,
        "line window",
        CodeUnitKind::LineWindow,
        slice.first().map_or(1, |(line, _)| *line),
        slice.last().map_or(1, |(line, _)| *line),
        &values,
      );
      unit.suppressed = suppressed;
      units.push(unit);
    }
  }
  units
}

/// Split normalized lines into segments of consecutive source lines.
fn line_segments(normalized: &[(usize, String)]) -> Vec<&[(usize, String)]> {
  crate::runs::split_runs_by(normalized, |prev, curr| curr.0 <= prev.0 + 1)
}

/// Merge adjacent blank-separated segments when both sides are uniform
/// declaration stanzas, so clap-style option blocks and blank-spread field
/// tables can form windows. A blank line stays a hard concept boundary
/// everywhere else.
fn coalesce_stanza_segments(segments: Vec<&[(usize, String)]>) -> Vec<Vec<(usize, String)>> {
  let mut merged: Vec<Vec<(usize, String)>> = Vec::new();
  for segment in segments {
    if let Some(last) = merged.last_mut()
      && segments_form_one_stanza_block(last, segment)
    {
      last.extend(segment.iter().cloned());
    } else {
      merged.push(segment.to_vec());
    }
  }
  merged
}

/// One blank line apart, both sides uniform declaration stanzas.
fn segments_form_one_stanza_block(prev: &[(usize, String)], next: &[(usize, String)]) -> bool {
  let Some((prev_end, _)) = prev.last() else {
    return false;
  };
  let Some((next_start, _)) = next.first() else {
    return false;
  };
  *next_start == prev_end + 2 && segment_is_declaration_stanza(prev) && segment_is_declaration_stanza(next)
}

/// The structural anatomy of a declaration block: a comment/attribute
/// prelude, at most one type-header row, uniform stanza rows, and at most
/// one trailing lone `}`. Prelude-only segments (comment banners, bare
/// attributes) and lone braces never qualify.
fn segment_is_declaration_stanza(slice: &[(usize, String)]) -> bool {
  let prelude = slice
    .iter()
    .take_while(|(_, line)| line_is_comment(line) || line_is_attribute(line))
    .count();
  let mut rows = &slice[prelude..];
  let has_header = rows.first().is_some_and(|(_, line)| line_is_type_header(line));
  if has_header {
    rows = &rows[1..];
  }
  if let Some((_, line)) = rows.last()
    && line.trim() == "}"
  {
    rows = &rows[..rows.len() - 1];
  }
  rows.iter().all(|(_, line)| line_is_stanza_shaped(line)) && (has_header || rows.iter().any(|(_, line)| line_is_structured_data(line)))
}

/// A type declaration's opening row (`pub struct Cli {`-style).
fn line_is_type_header(line: &str) -> bool {
  let trimmed = line.trim_start();
  starts_with_any(trimmed, &[
    "struct ", "pub struct ", "pub(crate) struct ", "enum ", "pub enum ", "union ",
  ]) && trimmed.trim_end().ends_with('{')
}

/// Reject windows that are the opening rows of a `fn` *declaration*.
///
/// Rust forces implementors to restate trait method signatures verbatim, so
/// a declaration's parameter rows always duplicate every implementation's
/// rows without describing duplicated behavior. The signature's terminator
/// decides which side of that pairing a window is on: looking ahead in the
/// segment, `;` marks the declaration (rejected) while a `{` body marks an
/// implementation (kept, so deliberate signature parity across implementors
/// stays visible).
fn line_window_is_declaration_signature_prefix(segment: &[(usize, String)], start: usize, len: usize) -> bool {
  let slice = &segment[start..start + len];
  let starts_like_fn = slice.first().is_some_and(|(_, line)| {
    let trimmed = line.trim_start();
    trimmed.starts_with("fn ") || trimmed.starts_with("pub fn ") || trimmed.starts_with("pub(crate) fn ")
  });
  if !starts_like_fn {
    return false;
  }
  if slice
    .iter()
    .any(|(_, line)| line.contains('{') || line.trim_end().ends_with(';'))
  {
    // The signature is terminated inside the window: a complete shape.
    return false;
  }
  segment[start + len..]
    .iter()
    .find_map(|(_, line)| {
      if line.contains('{') {
        Some(false)
      } else if line.trim_end().ends_with(';') {
        Some(true)
      } else {
        None
      }
    })
    .unwrap_or(false)
}

/// Reject windows whose edges are misaligned with concept boundaries.
fn line_window_is_aligned(slice: &[(usize, String)]) -> bool {
  let Some((_, first)) = slice.first() else {
    return false;
  };
  let Some((_, last)) = slice.last() else {
    return false;
  };
  !line_is_closing_only(first) && !line_ends_with_opener(last)
}

/// Return true for lines made only of closing delimiters and punctuation.
fn line_is_closing_only(line: &str) -> bool {
  !line.is_empty()
    && line
      .chars()
      .all(|ch| ch.is_whitespace() || matches!(ch, ')' | ']' | '}' | ';' | ','))
}

/// Return true for lines that end by opening a block whose body follows.
fn line_ends_with_opener(line: &str) -> bool {
  line.trim_end().ends_with(['{', '(', '[', ':'])
}

/// Strip `/* ... */` block-comment spans from one line, tracking open-comment
/// state across lines.
///
/// Comment markers are honored only outside quoted literals and line comments:
/// a quote span that closes on the same line is content, and `//` or `#`
/// turns the rest of the line into prose. Without this, a quoted `"/*"` (such
/// as the one in this function's own source) would open a phantom block
/// comment and blind line windowing to everything before the next stray `*/`.
fn strip_block_comments(line: &str, in_block_comment: &mut bool) -> String {
  let mut output = String::new();
  let mut chars = line.char_indices().peekable();

  while let Some((idx, ch)) = chars.next() {
    if *in_block_comment {
      if ch == '*' && chars.next_if(|&(_, next)| next == '/').is_some() {
        *in_block_comment = false;
      }
      continue;
    }
    match ch {
      '"' | '\'' if quote_span_closes_on_line(&line[idx..], ch) => {
        output.push(ch);
        while let Some((_, lit)) = chars.next() {
          output.push(lit);
          if lit == '\\' {
            if let Some((_, escaped)) = chars.next() {
              output.push(escaped);
            }
          } else if lit == ch {
            break;
          }
        }
      }
      '/' if chars.next_if(|&(_, next)| next == '*').is_some() => {
        *in_block_comment = true;
      }
      '/' if matches!(chars.peek(), Some(&(_, '/'))) => {
        output.push_str(&line[idx..]);
        break;
      }
      '#' => {
        output.push_str(&line[idx..]);
        break;
      }
      _ => output.push(ch),
    }
  }
  output
}

/// Whether the quote span opened at the start of `rest` closes on this line
/// (backslash escapes skipped). Unclosed openers — lifetimes, labels, prose
/// apostrophes, multi-line string heads — stay ordinary punctuation.
fn quote_span_closes_on_line(rest: &str, quote: char) -> bool {
  let mut chars = rest.chars().skip(1);
  while let Some(ch) = chars.next() {
    if ch == '\\' {
      chars.next();
    } else if ch == quote {
      return true;
    }
  }
  false
}

const fn line_span(line_start: usize, line_end: usize) -> usize {
  line_end.saturating_sub(line_start) + 1
}

/// Classify a token window against the suppression rules.
///
/// `None` means the window is a fully visible candidate; otherwise the first
/// matching enabled rule tags it. Disabled rules fall through, so toggling a
/// rule off makes its windows visible.
fn classify_token_window(segment: &[Token], slice: &[Token], mode: TokenMode, policy: &SuppressionPolicy) -> Option<RuleId> {
  if token_window_is_import_or_module_scaffold(slice) {
    return policy.allow(RuleId::TokenImportScaffold);
  }
  if token_window_is_chain_tail(slice) {
    return policy.allow(RuleId::TokenChainTail);
  }
  if token_window_is_signature_prefix(slice) {
    return policy.allow(RuleId::TokenSignaturePrefix);
  }
  if token_window_is_declaration_scaffold(slice) {
    return policy.allow(RuleId::TokenDeclarationScaffold);
  }
  if token_window_cuts_match_table_prefix(segment, slice.len()) {
    return policy.allow(RuleId::TokenMatchTablePrefix);
  }
  if token_window_scores_low_signal(slice, mode) {
    return policy.allow(RuleId::TokenLowSignal);
  }
  None
}

/// The meaningful/unique/behavior scoring fall-through of window eligibility.
fn token_window_scores_low_signal(slice: &[Token], mode: TokenMode) -> bool {
  let meaningful = slice
    .iter()
    .filter(|token| is_meaningful_token(token_value(token, mode)))
    .count();
  if meaningful < 5 || meaningful * 3 < slice.len() {
    return true;
  }

  let structured_data = token_window_has_structured_data(slice);
  let unique_values = unique_count(
    slice
      .iter()
      .map(|token| token_value(token, mode))
      .filter(|value| is_meaningful_token(value)),
  );
  if unique_values < 3 && !structured_data {
    return true;
  }

  let has_behavior = slice.iter().any(token_has_behavior);
  match mode {
    TokenMode::Normalized => !(has_behavior || structured_data),
    TokenMode::Raw => {
      !(has_behavior
        || structured_data
        || unique_count(
          slice
            .iter()
            .map(|token| token.raw.as_str())
            .filter(|value| is_meaningful_token(value)),
        ) >= 5)
    }
  }
}

fn token_value(token: &Token, mode: TokenMode) -> &str {
  match mode {
    TokenMode::Normalized => &token.normalized,
    TokenMode::Raw => &token.raw,
  }
}

fn token_window_is_import_or_module_scaffold(slice: &[Token]) -> bool {
  let has_import_or_module_line = window_token_lines(slice)
    .iter()
    .any(|line| line_starts_with(line, is_import_or_module_line_start));
  has_import_or_module_line && !slice.iter().any(token_has_behavior)
}

fn token_window_is_chain_tail(slice: &[Token]) -> bool {
  let lines = window_token_lines(slice);
  let chain_tail_lines = lines.iter().filter(|line| line_starts_with(line, is_chain_tail_start)).count();
  lines.len() >= 2 && chain_tail_lines * 2 >= lines.len() && !slice.iter().any(token_has_behavior)
}

/// True when the line's first raw token satisfies `starts`.
fn line_starts_with(line: &[Token], starts: impl Fn(&str) -> bool) -> bool {
  line.first().is_some_and(|token| starts(&token.raw))
}

/// Reject windows that stop part-way through a `match` arm table.
///
/// A minimum-satisfying prefix that cuts a long arm table pairs tables by
/// their shared opening rows even when the remaining arms differ; the table
/// is one concept and a prefix of it is not a reportable unit.
fn token_window_cuts_match_table_prefix(segment: &[Token], window_len: usize) -> bool {
  if segment.len() <= window_len {
    return false;
  }
  let window = &segment[..window_len];
  // Only `match` keyword tables of single-line `pattern => value,` rows:
  // arrow rows in macro invocations are complete data stanzas, and arms
  // that open blocks (`=> {`) are bodies rather than table rows.
  window.iter().any(|token| token.raw == "match")
    && window_token_lines(window).iter().any(|line| line_is_match_table_row(line))
    && window_token_lines(&segment[window_len..])
      .iter()
      .any(|line| line_is_match_table_row(line))
}

/// A one-line `pattern => value,` match-table row.
fn line_is_match_table_row(line: &[Token]) -> bool {
  has_match_arrow(line) && line.last().is_some_and(|token| token.raw == ",")
}

/// True when the tokens contain a `=>` arrow pair.
fn has_match_arrow(tokens: &[Token]) -> bool {
  tokens
    .windows(2)
    .any(|pair| pair[0].raw == "=" && pair[1].raw == ">" && pair[0].line == pair[1].line)
}

/// Reject windows that are mostly doc comments and an unfinished signature.
///
/// A documented multi-line `fn` signature tokenizes almost identically for
/// unrelated functions; a window must include real body content to stand for
/// a duplicate.
fn token_window_is_signature_prefix(slice: &[Token]) -> bool {
  // Only documented declarations: the window must begin at a doc comment
  // or attribute line. Windows that begin at code (imports, impl headers,
  // undocumented signatures) describe real adjacent content.
  if !slice.first().is_some_and(|token| token.raw == "/" || token.raw == "#") {
    return false;
  }
  if !slice.iter().any(|token| token.raw == "fn") {
    return false;
  }
  let scaffold_len = slice.iter().position(|token| token.raw == "{").unwrap_or(slice.len());
  if scaffold_len * 2 < slice.len() {
    return false;
  }
  // Only multi-line doc/parameter scaffolding counts; functions that open
  // their body on the signature line keep their windows.
  window_token_lines(&slice[..scaffold_len]).len() >= 3
}

/// Reject windows that are only a type declaration's field scaffolding.
///
/// A derive header plus rows of `name: Type,` fields repeats for every
/// config-style struct without describing duplicated behavior.
fn token_window_is_declaration_scaffold(slice: &[Token]) -> bool {
  let has_type_header = slice
    .iter()
    .any(|token| matches!(token.raw.as_str(), "struct" | "enum" | "trait" | "union"));
  if !has_type_header {
    return false;
  }
  let field_lines = window_token_lines(slice).iter().filter(|line| line_is_field_like(line)).count();
  field_lines >= 2 && !slice.iter().any(token_is_runtime_behavior)
}

/// `name: Type,` declaration rows.
fn line_is_field_like(line: &[Token]) -> bool {
  line.first().is_some_and(|token| token.normalized == "IDENT")
    && line.get(1).is_some_and(|token| token.raw == ":")
    && line.last().is_some_and(|token| token.raw == ",")
}

/// Tokens that indicate executable behavior rather than declaration shape.
fn token_is_runtime_behavior(token: &Token) -> bool {
  matches!(
    token.raw.as_str(),
    "fn" | "return" | "if" | "match" | "for" | "while" | "loop" | "let" | "=" | "+" | "*" | "%"
  )
}

fn token_has_behavior(token: &Token) -> bool {
  is_behavior_keyword(&token.raw) || BEHAVIOR_OPERATORS.contains(&token.raw.as_str())
}

fn token_window_has_structured_data(slice: &[Token]) -> bool {
  window_token_lines(slice)
    .iter()
    .filter(|line| line_has_structured_tokens(line))
    .count()
    >= 2
}

/// Group a token window into per-source-line token runs.
fn window_token_lines(slice: &[Token]) -> Vec<&[Token]> {
  crate::runs::split_runs_by(slice, |prev, curr| prev.line == curr.line)
}

fn line_has_structured_tokens(tokens: &[Token]) -> bool {
  tokens.iter().enumerate().any(|(index, token)| {
    token.raw == ":"
      && tokens[..index]
        .iter()
        .rev()
        .any(|candidate| is_meaningful_token(&candidate.normalized))
      && tokens[index + 1..]
        .iter()
        .any(|candidate| is_meaningful_token(&candidate.normalized))
  })
}

fn is_meaningful_token(value: &str) -> bool {
  !STRUCTURAL_TOKENS.contains(&value)
}

/// Tokens that are pure delimiters or punctuation with no content of their own.
const STRUCTURAL_TOKENS: &[&str] = &["{", "}", "(", ")", "[", "]", ",", ";", ":", ".", "#", "@", "\\"];

/// Operator tokens whose presence indicates computation rather than data.
const BEHAVIOR_OPERATORS: &[&str] = &["=", "+", "-", "*", "/", "%", "<", ">", "!", "&", "|", "^", "?"];

/// Keywords that mark a token or line as behavior rather than data; shared by
/// the token-window scorer and both line classifiers.
const BEHAVIOR_KEYWORDS: &[&str] = &[
  "async", "await", "break", "class", "const", "continue", "def", "else", "enum", "fn", "for", "if", "impl", "let", "loop", "match",
  "return", "static", "struct", "trait", "type", "where", "while", "yield",
];

fn is_import_or_module_line_start(value: &str) -> bool {
  matches!(value, "use" | "import" | "from" | "mod" | "pub" | "extern")
}

fn is_behavior_keyword(value: &str) -> bool {
  BEHAVIOR_KEYWORDS.contains(&value)
}

fn unique_count<'a>(values: impl Iterator<Item = &'a str>) -> usize {
  let mut seen = Vec::new();
  for value in values {
    if !seen.contains(&value) {
      seen.push(value);
    }
  }
  seen.len()
}

/// Classify a line window against the suppression rules.
///
/// `None` means the window is a fully visible candidate; otherwise the first
/// matching enabled rule tags it. Disabled rules fall through, so toggling a
/// rule off makes its windows visible.
fn classify_line_window(segment: &[(usize, String)], start: usize, len: usize, policy: &SuppressionPolicy) -> Option<RuleId> {
  let slice = &segment[start..start + len];
  // Admission rules first: a named carve-out turns an otherwise-rejected
  // window shape into a fully visible candidate; disabling the rule lets
  // the window fall through to its base suppression.
  if policy.is_enabled(RuleId::LineDeclarationStanza) && line_window_is_declaration_stanza(slice) {
    return None;
  }
  if policy.is_enabled(RuleId::LineBuilderChainRun) && line_window_is_builder_chain_run(slice) {
    return None;
  }
  if line_window_is_import_or_module_scaffold(slice) {
    return policy.allow(RuleId::LineImportScaffold);
  }
  if line_window_is_chain_tail(slice) {
    return policy.allow(RuleId::LineChainTail);
  }
  if line_window_is_declaration_signature_prefix(segment, start, len) {
    return policy.allow(RuleId::LineDeclarationSignaturePrefix);
  }
  if line_window_scores_low_signal(slice) {
    return policy.allow(RuleId::LineLowSignal);
  }
  None
}

/// A window made entirely of doc/attr/field declaration stanza lines with
/// real field content: cross-file copy-paste of clap-style option blocks and
/// derive field tables is duplication signal even though no line carries
/// behavior.
fn line_window_is_declaration_stanza(slice: &[(usize, String)]) -> bool {
  let structured_lines = slice.iter().filter(|(_, line)| line_is_structured_data(line)).count();
  structured_lines >= 2 && unique_terms_in_lines(slice) >= 4 && slice.iter().all(|(_, line)| line_is_stanza_shaped(line))
}

/// Declaration-stanza line shapes: doc comments, attributes, and field rows.
/// Block punctuation (a lone `{` or `}`) is never a stanza row.
///
/// Behavior keywords are checked by word (not `line_has_behavior`) because an
/// attribute's interior `=` is attribute syntax, not computation, and field
/// rows carry no operators.
fn line_is_stanza_shaped(line: &str) -> bool {
  if line_is_import_or_module_scaffold(line) {
    return false;
  }
  if matches!(line.trim(), "{" | "}") {
    return false;
  }
  if contains_word(line, BEHAVIOR_KEYWORDS) {
    return false;
  }
  line_is_comment(line) || line_is_attribute(line) || line_is_structured_data(line)
}

/// A window made entirely of complete single-line builder steps
/// (`.ident(args)` with balanced parens); detached fragments mixing other
/// lines keep their chain-tail suppression.
fn line_window_is_builder_chain_run(slice: &[(usize, String)]) -> bool {
  !slice.is_empty() && slice.iter().all(|(_, line)| line_is_builder_step(line))
}

/// A complete `.ident(...)` builder step on one line, optionally `,` or `;`
/// terminated.
fn line_is_builder_step(line: &str) -> bool {
  let trimmed = line.trim();
  let Some(rest) = trimmed.strip_prefix('.') else {
    return false;
  };
  let ident_len = rest.chars().take_while(|ch| is_ident_continue(*ch)).count();
  if ident_len == 0 {
    return false;
  }
  let after_ident = &rest[ident_len..];
  if !after_ident.starts_with('(') {
    return false;
  }
  let mut depth = 0usize;
  let mut close = None;
  for (idx, ch) in after_ident.char_indices() {
    match ch {
      '(' => depth += 1,
      ')' => {
        depth -= 1;
        if depth == 0 {
          close = Some(idx);
          break;
        }
      }
      _ => {}
    }
  }
  let Some(close) = close else {
    return false;
  };
  matches!(&after_ident[close + 1..], "" | "," | ";")
}

/// The meaningful/behavior/unique-terms scoring fall-through of eligibility.
fn line_window_scores_low_signal(slice: &[(usize, String)]) -> bool {
  let meaningful_lines = slice.iter().filter(|(_, line)| is_meaningful_line(line)).count();
  if meaningful_lines < 2 {
    return true;
  }

  let behavior_lines = slice.iter().filter(|(_, line)| line_has_behavior(line)).count();
  let prose_lines = slice.iter().filter(|(_, line)| line_is_prose(line)).count();
  let structured_lines = slice.iter().filter(|(_, line)| line_is_structured_data(line)).count();

  !((behavior_lines > 0 || prose_lines >= 2 || structured_lines >= 2) && unique_terms_in_lines(slice) >= 4)
}

fn line_window_is_import_or_module_scaffold(slice: &[(usize, String)]) -> bool {
  let import_or_module_lines = slice.iter().filter(|(_, line)| line_is_import_or_module_scaffold(line)).count();
  import_or_module_lines > 0
    && slice
      .iter()
      .all(|(_, line)| line_is_import_or_module_scaffold(line) || line_is_low_info(line))
}

fn line_window_is_chain_tail(slice: &[(usize, String)]) -> bool {
  let chain_tail_lines = slice.iter().filter(|(_, line)| line_is_chain_tail(line)).count();
  chain_tail_lines >= 2 && chain_tail_lines * 2 >= slice.len() && !slice.iter().any(|(_, line)| line_has_behavior(line))
}

fn is_meaningful_line(line: &str) -> bool {
  !line_is_low_info(line) && (line_has_behavior(line) || line_is_prose(line) || line_is_structured_data(line))
}

fn line_is_low_info(line: &str) -> bool {
  line_is_comment(line)
    || line_is_attribute(line)
    || line_is_delimiter_only(line)
    || line_is_import_or_module_scaffold(line)
    || line_is_chain_tail(line)
}

fn line_is_comment(line: &str) -> bool {
  let trimmed = line.trim_start();
  trimmed.starts_with("//") || trimmed.starts_with("///") || trimmed.starts_with("/*") || trimmed.starts_with("# ")
}

fn line_is_attribute(line: &str) -> bool {
  let trimmed = line.trim_start();
  trimmed.starts_with("#[") || trimmed.starts_with("#!") || trimmed.starts_with('@')
}

fn line_is_delimiter_only(line: &str) -> bool {
  line
    .chars()
    .all(|ch| ch.is_whitespace() || matches!(ch, '{' | '}' | '[' | ']' | '(' | ')' | ';' | ','))
}

fn line_is_import_or_module_scaffold(line: &str) -> bool {
  let trimmed = line.trim_start();
  starts_with_any(trimmed, &[
    "use ", "pub use ", "import ", "from ", "mod ", "pub mod ", "extern crate ",
  ])
}

fn line_is_chain_tail(line: &str) -> bool {
  let trimmed = line.trim_start();
  trimmed.starts_with('.') || matches!(trimmed, ")" | "};" | "});" | "];")
}

fn line_has_behavior(line: &str) -> bool {
  let trimmed = line.trim_start();
  contains_word(trimmed, BEHAVIOR_KEYWORDS)
    || trimmed.contains('=')
    || trimmed.contains("=>")
    || trimmed.contains("->")
    || trimmed.contains('+')
    || trimmed.contains('-')
    || trimmed.contains('*')
    || trimmed.contains('/')
    || trimmed.contains('%')
}

fn line_is_structured_data(line: &str) -> bool {
  let trimmed = line.trim_start();
  let Some((key, value)) = trimmed.split_once(':') else {
    return false;
  };
  has_structured_term(key) && has_structured_value(value)
}

fn has_structured_term(value: &str) -> bool {
  value
    .chars()
    .any(|ch| ch == '"' || ch == '\'' || ch == '_' || ch.is_ascii_alphanumeric())
}

fn has_structured_value(value: &str) -> bool {
  value
    .chars()
    .any(|ch| ch == '"' || ch == '\'' || ch == '#' || ch == '/' || ch == '.' || ch == '_' || ch.is_ascii_alphanumeric())
}

fn line_is_prose(line: &str) -> bool {
  !line.contains(['{', '}', ';', '='])
    && line
      .split(|ch: char| !ch.is_ascii_alphabetic())
      .filter(|word| word.len() >= 3)
      .count()
      >= 4
}

fn starts_with_any(value: &str, prefixes: &[&str]) -> bool {
  prefixes.iter().any(|prefix| value.starts_with(prefix))
}

/// Return true if `ch` is part of an identifier-like word.
const fn is_wordish_char(ch: char) -> bool {
  ch == '_' || ch.is_ascii_alphanumeric()
}

/// Split a line into identifier-like word parts.
fn wordish_parts(value: &str) -> impl Iterator<Item = &str> {
  value.split(|ch: char| !is_wordish_char(ch))
}

fn contains_word(value: &str, words: &[&str]) -> bool {
  words.iter().any(|word| wordish_parts(value).any(|part| part == *word))
}

fn unique_terms_in_lines(slice: &[(usize, String)]) -> usize {
  unique_count(
    slice
      .iter()
      .flat_map(|(_, line)| wordish_parts(line).filter(|term| term.len() >= 2)),
  )
}

fn is_chain_tail_start(value: &str) -> bool {
  matches!(value, "." | ")" | "]")
}

/// Return the text values of a generic window unit, if it is one.
///
/// Window bodies are flat blocks of token leaves; any other body shape
/// (for example synthetic test units) returns `None`.
#[must_use]
pub(crate) fn window_values(unit: &CodeUnit) -> Option<Vec<&str>> {
  if !matches!(unit.body.kind, NodeKind::Block) {
    return None;
  }
  unit
    .body
    .children
    .iter()
    .map(|child| match &child.kind {
      NodeKind::Token(value) if child.children.is_empty() => Some(value.as_str()),
      _ => None,
    })
    .collect()
}

/// Build a code unit from generic text values.
pub(crate) fn window_unit(path: &Path, name: &str, kind: CodeUnitKind, line_start: usize, line_end: usize, values: &[String]) -> CodeUnit {
  let body = NormalizedNode::with_children(
    NodeKind::Block,
    values
      .iter()
      .map(|value| NormalizedNode::leaf(NodeKind::Token(value.clone())))
      .collect(),
  );
  CodeUnit {
    suppressed: None,
    parent_chain: None,
    kind,
    name: name.to_string(),
    file: path.to_path_buf(),
    line_start,
    line_end,
    signature: NormalizedNode::leaf(NodeKind::Opaque),
    fingerprint: Fingerprint::from_node(&body),
    node_count: values.len(),
    body,
    parent_name: None,
    is_test: false,
  }
}

#[cfg(test)]
mod tests {
  use std::collections::BTreeSet;

  use super::*;

  #[test]
  fn tokenize_consumes_identifier_and_numeric_tails() {
    let tokens = tokenize("alpha_2 beta-x 31.5_f32", QuoteProfile::default());
    let raws: Vec<_> = tokens.iter().map(|token| token.raw.as_str()).collect();
    assert_eq!(raws, vec!["alpha_2", "beta-x", "31.5_f32"]);
    let normalized: Vec<_> = tokens.iter().map(|token| token.normalized.as_str()).collect();
    assert_eq!(normalized, vec!["IDENT", "IDENT", "NUMBER"]);
  }

  #[test]
  fn rust_ticks_lex_char_literals_and_leave_lifetimes_as_punctuation() {
    let rust = QuoteProfile {
      rust_ticks: true
    };
    let tokens = tokenize("fn f<'a>(x: &'a str) -> char { 'x' }", rust);
    let strings: Vec<&str> = tokens
      .iter()
      .filter(|token| token.normalized == "STRING")
      .map(|token| token.raw.as_str())
      .collect();
    assert_eq!(strings, vec!["'x'"]);
    assert_eq!(tokens.iter().filter(|token| token.raw == "'").count(), 2);
  }

  #[test]
  fn rust_ticks_pair_escaped_char_literals() {
    let rust = QuoteProfile {
      rust_ticks: true
    };
    for literal in ["'\\n'", "'\\''", "'\\u{1F600}'"] {
      let tokens = tokenize(literal, rust);
      assert_eq!(tokens.len(), 1, "{literal} should lex as one token");
      assert_eq!(tokens[0].normalized, "STRING");
      assert_eq!(tokens[0].raw, literal);
    }
  }

  #[test]
  fn default_profile_keeps_naive_apostrophe_pairing() {
    let tokens = tokenize("x = 'hello world'", QuoteProfile::default());
    assert!(
      tokens
        .iter()
        .any(|token| token.normalized == "STRING" && token.raw == "'hello world'")
    );
  }

  #[test]
  fn comment_apostrophes_do_not_bridge_token_segments() {
    // The defect the Rust profile fixes: an unpaired tick (lifetime or
    // prose apostrophe) used to open a phantom multi-line string that
    // bridged blank lines and swallowed all later segments.
    let rust = QuoteProfile {
      rust_ticks: true
    };
    let tokens = tokenize("// doesn't pair\nlet a = 1;\n\nlet b = 2;\n", rust);
    assert!(tokens.iter().all(|token| token.line == token.end_line));
    assert_eq!(token_segments(&tokens).len(), 2);
  }

  #[test]
  fn lifetime_ticks_do_not_blind_token_windows() {
    // Three ticks (two lifetimes, one comment apostrophe) precede the
    // duplicated segments; the old naive pairing left one tick unpaired
    // and swallowed everything after it out of token segmentation.
    let source = "\
fn keep<'a>(x: &'a str) -> String {
    // it doesn't allocate much
    x.to_string()
}

fn alpha(items: &[u32]) -> Vec<u32> {
    items.iter().map(|v| v + 1).collect()
}

fn beta(items: &[u32]) -> Vec<u32> {
    items.iter().map(|v| v + 1).collect()
}
";
    let config = Config {
      token_min_tokens: 10,
      token_min_lines: 2,
      ..Config::default()
    };
    let units = extract(Path::new("src/lib.rs"), source, &config);
    let starts: Vec<usize> = units.normalized_tokens.iter().map(|unit| unit.line_start).collect();
    assert!(
      starts.contains(&6) && starts.contains(&10),
      "windows must cover the segments after the ticks: {starts:?}"
    );
  }

  #[test]
  fn wordish_parts_split_on_non_word_characters() {
    let parts: Vec<_> = wordish_parts("alpha_one,beta.two(three)")
      .filter(|part| !part.is_empty())
      .collect();
    assert_eq!(parts, vec!["alpha_one", "beta", "two", "three"]);
  }

  #[test]
  fn structured_data_counts_the_final_window_line() {
    // The last line of a window participates in per-line aggregation
    // exactly like interior lines.
    let tokens = tokenize("alpha: 1\nbeta: 2", QuoteProfile::default());
    assert!(token_window_has_structured_data(&tokens));
  }

  #[test]
  fn disabled_token_dimensions_produce_no_token_windows() {
    let config = Config {
      token_min_tokens: 8,
      token_min_lines: 1,
      enabled_dimensions: BTreeSet::from([DetectionDimension::Line]),
      ..Config::default()
    };
    let units = extract(Path::new("sample.rs"), "fn a() {\nlet x = 1;\nreturn x + 1;\n}", &config);
    assert!(units.normalized_tokens.is_empty());
    assert!(units.raw_tokens.is_empty());
  }

  fn normalized_token_windows_for(source: &str, min_tokens: usize) -> Vec<CodeUnit> {
    let config = Config {
      token_min_tokens: min_tokens,
      token_min_lines: 2,
      ..Config::default()
    };
    extract(Path::new("sample.rs"), source, &config).normalized_tokens
  }

  fn line_windows_for(source: &str, min_lines: usize) -> Vec<CodeUnit> {
    let config = Config {
      line_min_lines: min_lines,
      ..Config::default()
    };
    extract(Path::new("sample.rs"), source, &config).lines
  }

  #[track_caller]
  fn assert_all_tagged(units: &[CodeUnit], rule: RuleId) {
    assert!(!units.is_empty(), "windows must exist to carry the tag");
    for unit in units {
      assert_eq!(unit.suppressed, Some(rule), "window at {}", unit.line_start);
    }
  }

  // jscpd:ignore-start

  #[test]
  fn token_windows_reject_scaffolding_prefixes() {
    // A cut `match` arm table, a documented multi-line signature, and a
    // struct declaration: none may become standalone windows.
    let match_table_prefix = "\
fn map_binary(op: &Op) -> Kind {
    match op {
        Op::Add(_) => Kind::Add,
        Op::Sub(_) => Kind::Sub,
        Op::Mul(_) => Kind::Mul,
        Op::Div(_) => Kind::Div,
        Op::Rem(_) => Kind::Rem,
    }
}
";
    let documented_signature = "\
/// Normalize one construct from the parse tree rows.
/// Produces a normalized node for the duplicate pipeline.
fn normalize_construct(
    node: tree_sitter::Node,
    source: &[u8],
    mapping: &NodeMapping,
    ctx: &mut NormalizationContext,
) -> NormalizedNode {
";
    let declaration_scaffold = "\
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct DimensionConfig {
    ast: Option<bool>,
    sub_ast: Option<bool>,
    token_normalized: Option<bool>,
    token_raw: Option<bool>,
}
";
    for (min_tokens, source) in [(40, match_table_prefix), (30, documented_signature), (20, declaration_scaffold)] {
      let config = Config {
        token_min_tokens: min_tokens,
        token_min_lines: 2,
        ..Config::default()
      };
      let units = extract(Path::new("sample.rs"), source, &config);
      assert!(
        units.normalized_tokens.iter().all(|unit| unit.suppressed.is_some()),
        "scaffolding window must be tagged for:\n{source}"
      );
    }
  }

  #[test]
  fn token_window_scaffolding_rules_attribute_their_shapes() {
    let policy = SuppressionPolicy::default();
    let attribute = |source: &str, min_tokens: usize| {
      let tokens = tokenize(source, QuoteProfile::default());
      let segment = token_segments(&tokens)[0];
      let slice = anchored_window(segment, min_tokens, 2).expect("window exists");
      classify_token_window(segment, slice, TokenMode::Normalized, &policy)
    };
    let match_table_prefix = "\
fn map_binary(op: &Op) -> Kind {
    match op {
        Op::Add(_) => Kind::Add,
        Op::Sub(_) => Kind::Sub,
        Op::Mul(_) => Kind::Mul,
        Op::Div(_) => Kind::Div,
        Op::Rem(_) => Kind::Rem,
    }
}
";
    assert_eq!(attribute(match_table_prefix, 40), Some(RuleId::TokenMatchTablePrefix));
    let documented_signature = "\
/// Normalize one construct from the parse tree rows.
/// Produces a normalized node for the duplicate pipeline.
fn normalize_construct(
    node: tree_sitter::Node,
    source: &[u8],
    mapping: &NodeMapping,
    ctx: &mut NormalizationContext,
) -> NormalizedNode {
    normalize(node, source, mapping, ctx)
}
";
    assert_eq!(attribute(documented_signature, 30), Some(RuleId::TokenSignaturePrefix));
    let declaration_scaffold = "\
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct DimensionConfig {
    ast: Option<bool>,
    sub_ast: Option<bool>,
    token_normalized: Option<bool>,
    token_raw: Option<bool>,
}
";
    // 35 tokens span the derive header (21) plus two complete field rows
    // (7 each); a shorter window cuts the second row and falls through to
    // the low-signal score or, with structured data, stays visible.
    assert_eq!(attribute(declaration_scaffold, 35), Some(RuleId::TokenDeclarationScaffold));
  }

  // jscpd:ignore-end

  #[test]
  fn token_windows_keep_behavior_bearing_scaffold_counterexamples() {
    let block_arms = "\
fn map_binary(op: Op, value: i32) -> Kind {
    match op {
        Op::Add => {
            let next = value + 1;
            Kind::Add(next)
        }
        Op::Sub => {
            let next = value - 1;
            Kind::Sub(next)
        }
    }
}
";
    let declaration_with_impl = "\
struct Counter {
    total: i32,
    limit: i32,
}

impl Counter {
    fn clamp(&self, input: i32) -> i32 {
        let bounded = input + self.total;
        if bounded > self.limit { self.limit } else { bounded }
    }
}
";
    let complete_chain = "\
fn collect_names(items: &[Item]) -> Vec<String> {
    let names = items
        .iter()
        .map(|item| item.name.to_string())
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    names
}
";

    for (source, min_tokens, label) in [
      (block_arms, 30, "match arms with block bodies carry behavior"),
      (declaration_with_impl, 25, "declaration-adjacent runtime behavior stays eligible"),
      (complete_chain, 30, "complete callback chains stay eligible as token windows"),
    ] {
      assert!(!normalized_token_windows_for(source, min_tokens).is_empty(), "{label}");
    }
  }

  #[test]
  fn line_windows_reject_chain_tails_as_standalone_windows() {
    let tail_only = "\
    .iter()
    .map(|item| item.name.to_string())
    .filter(|name| !name.is_empty())
    .collect::<Vec<_>>();
";

    assert_all_tagged(&line_windows_for(tail_only, 4), RuleId::LineChainTail);
  }

  #[test]
  fn line_windows_split_declaration_and_impl_signatures() {
    // Rust forces implementors to restate trait signatures, so only the
    // declaration side (terminated by `;`) loses its window; the
    // implementation side (a `{` body follows) keeps deliberate parity
    // visible.
    let declaration = "\
trait Reporter {
    fn report_groups(
        &self,
        groups: &[DuplicateGroup],
        writer: &mut dyn Write,
        section: ReportSection,
    ) -> Result<()>;
}
";
    let implementation = "\
fn parse_file(
    &self,
    path: &Path,
    source: &str,
    config: &AnalysisConfig,
) -> Result<Vec<CodeUnit>, Error> {
    parse_source(path, source, config.min_nodes, config.min_lines)
}
";
    let config = Config {
      line_min_lines: 5,
      ..Config::default()
    };
    let declaration_units = extract(Path::new("decl.rs"), declaration, &config);
    let implementation_units = extract(Path::new("impl.rs"), implementation, &config);
    assert!(
      declaration_units
        .lines
        .iter()
        .filter(|unit| unit.line_start == 2)
        .all(|unit| unit.suppressed == Some(RuleId::LineDeclarationSignaturePrefix)),
      "the declaration's `fn` row window must carry the prefix tag"
    );
    assert!(
      implementation_units
        .lines
        .iter()
        .any(|unit| unit.line_start == 1 && unit.suppressed.is_none()),
      "implementation signature rows keep their window visible"
    );
  }

  #[test]
  fn token_windows_keep_complete_code_stanzas() {
    // Arrow rows inside a macro invocation are a complete data stanza,
    // and windows that begin at code (an undocumented impl signature)
    // describe real content; neither is scaffolding.
    let macro_arrow_table = "\
support_tests! {
    common::binary;
    check_passes_with_defaults => support::check_passes_with_defaults;
    check_fails_with_duplicates => support::check_fails_with_duplicates;
    check_reports_thresholds => support::check_reports_thresholds;
    check_handles_empty_input => support::check_handles_empty_input;
}
";
    let undocumented_signature = "\
fn parse_file(
    &self,
    path: &Path,
    source: &str,
    config: &AnalysisConfig,
) -> Result<Vec<CodeUnit>, Error> {
    parser::parse_source(path, source, config.min_nodes, config.min_lines)
}
";
    for source in [macro_arrow_table, undocumented_signature] {
      let config = Config {
        token_min_tokens: 30,
        token_min_lines: 2,
        ..Config::default()
      };
      let units = extract(Path::new("sample.rs"), source, &config);
      assert!(
        !units.normalized_tokens.is_empty(),
        "complete stanza must keep its window for:\n{source}"
      );
    }
  }

  #[test]
  fn normalized_tokens_ignore_identifier_names() {
    let first = tokenize("fn alpha(x: i32) { let beta = x + 1; }", QuoteProfile::default());
    let second = tokenize("fn gamma(y: i32) { let delta = y + 1; }", QuoteProfile::default());
    let first_norm: Vec<_> = first.iter().map(|token| token.normalized.as_str()).collect();
    let second_norm: Vec<_> = second.iter().map(|token| token.normalized.as_str()).collect();
    assert_eq!(first_norm, second_norm);
  }

  #[test]
  fn token_windows_use_source_lines() {
    let config = Config {
      token_min_tokens: 8,
      token_min_lines: 1,
      ..Config::default()
    };
    let units = extract(Path::new("sample.rs"), "fn a() {\nlet x = 1;\nreturn x + 1;\n}", &config);
    assert!(!units.normalized_tokens.is_empty());
    assert!(units.normalized_tokens[0].line_end >= units.normalized_tokens[0].line_start);
  }

  #[test]
  fn line_windows_normalize_whitespace() {
    let config = Config {
      line_min_lines: 2,
      ..Config::default()
    };
    let units = extract(
      Path::new("README.md"),
      "alpha computes the totals today\n bravo records the outputs tomorrow \n",
      &config,
    );
    assert_eq!(units.lines.len(), 1);
    assert_eq!(units.lines[0].line_start, 1);
    assert_eq!(units.lines[0].line_end, 2);
    let values = window_values(&units.lines[0]).unwrap();
    assert_eq!(values[1], "bravo records the outputs tomorrow");
  }

  #[test]
  fn line_windows_do_not_cross_blank_line_boundaries() {
    let config = Config {
      line_min_lines: 4,
      ..Config::default()
    };
    // Two unrelated concepts: the tail of the first and the head of the
    // second must not be stitched into one window across the blank line.
    let source = "\
alpha computes the totals from rows
bravo validates the threshold value

charlie records the result for callers
delta emits the summary into reports
";
    let units = extract(Path::new("sample.txt"), source, &config);
    assert!(units.lines.is_empty());
  }

  /// Build a segment slice of consecutive numbered lines.
  fn numbered_lines(start: usize, lines: &[&str]) -> Vec<(usize, String)> {
    lines
      .iter()
      .enumerate()
      .map(|(offset, line)| (start + offset, (*line).to_string()))
      .collect()
  }

  #[test]
  fn stanza_segment_accepts_attribute_prelude_before_header() {
    // The real clap/derive shape: doc and derive rows sit above the type
    // header.
    let segment = numbered_lines(1, &[
      "/// Spread option table.", "#[derive(Debug, Default)]", "pub struct SpreadA {", "pub alpha: usize,",
    ]);
    assert!(segment_is_declaration_stanza(&segment));
  }

  #[test]
  fn stanza_segment_accepts_trailing_close_brace_row() {
    // The final stanza of a declaration block carries the block's `}`.
    let segment = numbered_lines(10, &["/// Final option.", "#[arg(long)]", "pub omega: bool,", "}"]);
    assert!(segment_is_declaration_stanza(&segment));
  }

  #[test]
  fn stanza_segment_rejects_lone_close_brace() {
    let segment = numbered_lines(20, &["}"]);
    assert!(!segment_is_declaration_stanza(&segment));
  }

  #[test]
  fn stanza_segment_rejects_comment_banner_without_rows() {
    let segment = numbered_lines(1, &[
      "// ------------------------------------",
      "// Configuration",
      "// ------------------------------------",
    ]);
    assert!(!segment_is_declaration_stanza(&segment));
  }

  #[test]
  fn stanza_segment_rejects_import_rows() {
    let segment = numbered_lines(1, &["use std::collections::BTreeMap;", "use std::path::PathBuf;"]);
    assert!(!segment_is_declaration_stanza(&segment));
  }

  #[test]
  fn stanza_block_merges_derive_headed_blank_separated_table() {
    // The doc/derive prelude above the header and the trailing brace row
    // must not break a blank-spread field table apart.
    let first = numbered_lines(1, &[
      "/// Spread option table.", "#[derive(Debug, Default)]", "pub struct SpreadA {", "pub alpha: usize,",
    ]);
    let second = numbered_lines(6, &["pub beta: usize,", "}"]);
    let merged = coalesce_stanza_segments(vec![first.as_slice(), second.as_slice()]);
    assert_eq!(merged.len(), 1, "blank-separated table must coalesce");
  }

  #[test]
  fn stanza_block_merges_final_stanza_before_close() {
    // The last stanza of a clap-style struct ends with the block's `}`;
    // it must still join the stanza above it.
    let first = numbered_lines(1, &["/// First option.", "#[arg(long)]", "pub alpha: usize,"]);
    let second = numbered_lines(5, &["/// Final option.", "#[arg(long)]", "pub omega: bool,", "}"]);
    let merged = coalesce_stanza_segments(vec![first.as_slice(), second.as_slice()]);
    assert_eq!(merged.len(), 1, "final stanza must join the block");
  }

  #[test]
  fn stanza_window_rejects_brace_only_lines() {
    // Block punctuation inside a window is not a declaration row, so the
    // window must not be admitted as a stanza.
    let window = numbered_lines(1, &["pub alpha: usize,", "pub beta: usize,", "}"]);
    assert!(!line_window_is_declaration_stanza(&window));
  }

  // jscpd:ignore-start

  #[test]
  fn line_windows_anchor_to_concept_segments() {
    let config = Config {
      line_min_lines: 3,
      ..Config::default()
    };
    let source = "\
let total = first + second;
let result = total * scale;
return result + offset;

alpha computes the totals from rows
bravo validates the threshold value
charlie records the result for callers
";
    let units = extract(Path::new("sample.rs"), source, &config);
    let spans: Vec<_> = units.lines.iter().map(|u| (u.line_start, u.line_end)).collect();
    assert_eq!(spans, vec![(1, 3), (5, 7)]);
  }

  // jscpd:ignore-end

  #[test]
  fn line_windows_do_not_end_on_block_opening_lines() {
    let config = Config {
      line_min_lines: 3,
      ..Config::default()
    };
    // A window ending on the `fn ... {` line would cut into a body that
    // diverges immediately after the signature.
    let source = "\
let total = first + second;
let result = total * scale;
fn helper(value: i32) -> i32 {
let shifted = value + offset;
return shifted * scale;
}
";
    let units = extract(Path::new("sample.rs"), source, &config);
    assert!(
      units.lines.iter().all(|unit| unit.line_end != 3 && unit.line_start != 6),
      "no window may end on the opener line or start on the closer line"
    );
    assert!(
      units.lines.iter().any(|unit| (unit.line_start, unit.line_end) == (3, 5)),
      "the signature plus its own body is still a valid window"
    );
  }

  #[test]
  fn token_windows_do_not_cross_blank_line_boundaries() {
    let config = Config {
      token_min_tokens: 16,
      token_min_lines: 2,
      ..Config::default()
    };
    // Each concept alone is below the token minimum; only a window
    // stitched across the blank line would reach it.
    let source = "\
let total = first + second;
let result = total * scale;

let shifted = value + offset;
let scaled = shifted * factor;
";
    let units = extract(Path::new("sample.rs"), source, &config);
    assert!(units.normalized_tokens.is_empty());
    assert!(units.raw_tokens.is_empty());
  }

  #[test]
  fn token_windows_are_stable_across_unrelated_line_shifts() {
    let config = Config {
      token_min_tokens: 12,
      token_min_lines: 3,
      ..Config::default()
    };
    let concept = "\
fn total(values: &[i32]) -> i32 {
    let mut sum = 0;
    for value in values {
        sum = sum + value;
    }
    return sum;
}
";
    let shifted = format!("let unrelated = prefix_offset(1);\n\n{concept}");
    let original = extract(Path::new("first.rs"), concept, &config);
    let moved = extract(Path::new("second.rs"), &shifted, &config);
    let original_fps: Vec<_> = original.normalized_tokens.iter().map(|u| u.fingerprint).collect();
    assert!(!original_fps.is_empty());
    assert!(
      moved.normalized_tokens.iter().any(|u| original_fps.contains(&u.fingerprint)),
      "line-anchored windows must keep matching after unrelated lines shift the concept"
    );
  }

  #[test]
  fn token_windows_respect_min_line_span() {
    let dense = "fn a() { let x = 1; let y = 2; let z = x + y; }";
    let config = Config {
      token_min_tokens: 8,
      token_min_lines: 2,
      ..Config::default()
    };
    let units = extract(Path::new("sample.rs"), dense, &config);
    assert!(units.normalized_tokens.is_empty());
  }

  // jscpd:ignore-start

  #[test]
  fn line_windows_include_shifted_duplicate_candidates() {
    let config = Config {
      line_min_lines: 3,
      ..Config::default()
    };
    let source = "\
alpha computes the total from account rows
bravo validates the threshold before export
charlie records the result for the caller
delta emits the summary into the report
echo preserves the fallback for empty input
foxtrot keeps the warnings near the output
golf returns the final status to clients
";
    let units = extract(Path::new("sample.txt"), source, &config);
    let spans: Vec<_> = units.lines.iter().map(|u| (u.line_start, u.line_end)).collect();
    assert_eq!(spans, vec![(1, 3), (2, 4), (3, 5), (4, 6), (5, 7)]);
  }

  // jscpd:ignore-end

  #[test]
  fn token_windows_reject_import_module_scaffolding() {
    let config = Config {
      token_min_tokens: 12,
      token_min_lines: 2,
      ..Config::default()
    };
    let source = "\
use crate::alpha::Beta;
use crate::gamma::Delta;
pub mod tests;
use super::*;
";
    let units = extract(Path::new("sample.rs"), source, &config);
    assert_all_tagged(&units.normalized_tokens, RuleId::TokenImportScaffold);
    assert_all_tagged(&units.raw_tokens, RuleId::TokenImportScaffold);
  }

  // jscpd:ignore-start

  #[test]
  fn token_windows_preserve_substantive_executable_code() {
    let config = Config {
      token_min_tokens: 12,
      token_min_lines: 3,
      ..Config::default()
    };
    let source = "\
fn total(values: &[i32]) -> i32 {
    let mut sum = 0;
    for value in values {
        sum = sum + value;
    }
    return sum;
}
";
    let units = extract(Path::new("sample.rs"), source, &config);
    assert!(!units.normalized_tokens.is_empty());
    assert!(!units.raw_tokens.is_empty());
  }

  // jscpd:ignore-end

  #[test]
  fn token_windows_preserve_structured_data_blocks() {
    let config = Config {
      token_min_tokens: 12,
      token_min_lines: 3,
      ..Config::default()
    };
    let source = r#"{
  "service": "api",
  "timeout": 30,
  "endpoint": "/v1/items",
  "team": "platform"
}"#;
    let units = extract(Path::new("sample.json"), source, &config);
    assert!(!units.normalized_tokens.is_empty());
    assert!(!units.raw_tokens.is_empty());
  }

  #[test]
  fn token_windows_reject_method_chain_tails() {
    let config = Config {
      token_min_tokens: 12,
      token_min_lines: 3,
      ..Config::default()
    };
    let source = "\
command()
    .arg(\"report\")
    .arg(\"--format\")
    .arg(\"json\")
    .assert()
    .success();
";
    let units = extract(Path::new("sample.rs"), source, &config);
    assert_all_tagged(&units.normalized_tokens, RuleId::TokenChainTail);
    assert_all_tagged(&units.raw_tokens, RuleId::TokenChainTail);
  }

  #[test]
  fn line_windows_reject_import_module_scaffolding() {
    let config = Config {
      line_min_lines: 5,
      ..Config::default()
    };
    let source = "\
use crate::alpha::Beta;
use crate::gamma::Delta;
#[cfg(test)]
mod tests {
}
";
    let units = extract(Path::new("sample.rs"), source, &config);
    assert_all_tagged(&units.lines, RuleId::LineImportScaffold);
  }

  #[test]
  fn line_windows_reject_method_chain_tails() {
    // No `-` anywhere: a dash counts as line behavior, which routes a
    // window to the low-signal score instead of the chain-tail rule.
    let config = Config {
      line_min_lines: 5,
      ..Config::default()
    };
    let source = "\
command()
    .arg(\"report\")
    .arg(\"json\")
    .assert()
    .success();
";
    let units = extract(Path::new("sample.rs"), source, &config);
    assert_all_tagged(&units.lines, RuleId::LineChainTail);
  }

  // jscpd:ignore-start

  #[test]
  fn line_windows_preserve_substantive_executable_code() {
    let config = Config {
      line_min_lines: 5,
      ..Config::default()
    };
    let source = "\
fn total(values: &[i32]) -> i32 {
    let mut sum = 0;
    for value in values {
        sum = sum + value;
    }
    return sum;
}
";
    let units = extract(Path::new("sample.rs"), source, &config);
    assert!(!units.lines.is_empty());
  }

  // jscpd:ignore-end

  #[test]
  fn line_windows_preserve_substantive_text_blocks() {
    let config = Config {
      line_min_lines: 5,
      ..Config::default()
    };
    let source = "\
alpha computes the total from account rows
bravo validates the threshold before export
charlie records the result for the caller
delta emits the summary into the report
echo preserves the fallback for empty input
";
    let units = extract(Path::new("sample.txt"), source, &config);
    assert_eq!(units.lines.len(), 1);
  }

  #[test]
  fn line_windows_preserve_structured_data_blocks() {
    let config = Config {
      line_min_lines: 5,
      ..Config::default()
    };
    let source = "\
service: api
timeout: 30
endpoint: /v1/items
team: platform
region: us-east
";
    let units = extract(Path::new("sample.yaml"), source, &config);
    assert_eq!(units.lines.len(), 1);
  }

  #[test]
  fn line_windows_preserve_markdown_bullet_prose() {
    let config = Config {
      line_min_lines: 5,
      ..Config::default()
    };
    let source = "\
* validate incoming records before exporting results
* preserve warning details for later diagnosis
* compare generated reports with expected output
* collect duplicate groups for human review
* document policy decisions after each audit
";
    let units = extract(Path::new("README.md"), source, &config);
    assert_eq!(units.lines.len(), 1);
  }

  #[test]
  fn line_windows_reject_block_comment_only_prose() {
    let config = Config {
      line_min_lines: 5,
      ..Config::default()
    };
    let source = "\
/*
 * validate incoming records before exporting results
 * preserve warning details for later diagnosis
 * compare generated reports with expected output
 * collect duplicate groups for human review
 */
";
    let units = extract(Path::new("sample.rs"), source, &config);
    assert!(units.lines.is_empty());
  }

  #[test]
  fn comment_marker_lines_do_not_blind_line_windows() {
    let config = Config {
      line_min_lines: 3,
      ..Config::default()
    };
    // A `/*` inside a string literal or after `//` is content, not a
    // block-comment opener: the lines after it must still window.
    let quoted_marker = "\
let marker = rest.find(\"/*\");
let total = alpha + beta;
let scaled = total * gamma;
let bounded = scaled - delta;
";
    let line_comment_marker = "\
// tracks spans like /* these
let sum = one + two + three;
let widened = sum * sum;
let clamped = widened / four;
";
    for source in [quoted_marker, line_comment_marker] {
      let units = extract(Path::new("sample.rs"), source, &config);
      assert!(
        units.lines.iter().any(|unit| unit.line_end == 4),
        "line windows after a non-comment marker must still exist for:\n{source}"
      );
    }
  }

  #[test]
  fn strip_block_comments_tracks_comment_state_across_lines() {
    let mut in_comment = false;
    let sequence = [
      ("alpha /* gone */ beta", "alpha  beta", false),
      ("open /* spans", "open ", true),
      ("still hidden", "", true),
      ("done */ tail", " tail", false),
    ];
    for (line, expected, state_after) in sequence {
      assert_eq!(strip_block_comments(line, &mut in_comment), expected);
      assert_eq!(in_comment, state_after, "comment state after {line:?}");
    }
  }

  #[test]
  fn strip_block_comments_keeps_quoted_and_prose_markers() {
    let mut in_comment = false;
    assert_eq!(strip_block_comments("rest.find(\"/*\")", &mut in_comment), "rest.find(\"/*\")");
    assert!(!in_comment);
    assert_eq!(
      strip_block_comments("let quote = '\"'; /* gone */", &mut in_comment),
      "let quote = '\"'; "
    );
    assert!(!in_comment);
    assert_eq!(strip_block_comments("// prose /* stays", &mut in_comment), "// prose /* stays");
    assert!(!in_comment);
    assert_eq!(strip_block_comments("# python /* stays", &mut in_comment), "# python /* stays");
    assert!(!in_comment);
    assert_eq!(
      strip_block_comments("fn f<'a>(s: &'a str) { /* gone */ }", &mut in_comment),
      "fn f<'a>(s: &'a str) {  }"
    );
    assert!(!in_comment);
  }
}
