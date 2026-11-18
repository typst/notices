use std::fmt::{self, Display, Formatter};

use spdx::{LicenseItem, LicenseReq};
use unscanny::Scanner;

use crate::rules::{
    self, HEADER, INCLUSIONS, ITEM_START, MARKER_WORDS, MIN_MARKER_LEN, PROPERTY_STARTS,
    Verbatim, Violation,
};
use crate::{Inclusion, Item, License, LicenseBlock, Package};

/// An error that occurred while parsing a Typst NOTICE file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub kind: ErrorKind,
    /// The byte offset in the text at which the error occurred.
    pub index: usize,
    /// The 1-based line at which the error occurred.
    pub line: usize,
    /// The 1-based column (counted in characters) at which the error occurred.
    pub column: usize,
    /// A description of what is wrong and, where possible, how to fix it.
    pub message: String,
}

/// The kind of a [`ParseError`].
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// The file does not start with the exact Typst NOTICE header.
    MissingHeader,
    /// The file starts with the Typst NOTICE header, but violates the format.
    Malformed,
}

impl ParseError {
    /// Creates an error pointing at the byte `index` in `text`.
    pub(crate) fn new(
        kind: ErrorKind,
        text: &str,
        index: usize,
        message: impl Into<String>,
    ) -> Self {
        let before = &text[..index];
        let line = before.matches('\n').count() + 1;
        let line_start = before.rfind('\n').map_or(0, |i| i + 1);
        let column = before[line_start..].chars().count() + 1;
        Self { kind, index, line, column, message: message.into() }
    }
}

impl ParseError {
    /// Appends a hint at a possible cause to the message.
    pub(crate) fn with_hint(mut self, hint: &str) -> Self {
        self.message = format!("{}; {hint}", self.message);
        self
    }
}

impl Display for ParseError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}

impl std::error::Error for ParseError {}

type Result<T> = std::result::Result<T, ParseError>;

/// Parses a Typst NOTICE file.
pub(crate) fn parse(text: &str) -> Result<Vec<LicenseBlock>> {
    let mut parser = Parser::new(text);
    if !parser.header() {
        return Err(ParseError::new(
            ErrorKind::MissingHeader,
            text,
            0,
            "the file does not start with the Typst NOTICE header",
        ));
    }
    parser.newline()?;
    parser.newline()?;
    parser.blocks()
}

/// A parser for Typst NOTICE files.
struct Parser<'a> {
    /// The scanner: contains the underlying text and location as a "cursor".
    s: Scanner<'a>,
    /// The linebreak used by the file, as determined by its first linebreak.
    newline: &'static str,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        let newline = match text.find(['\r', '\n']) {
            Some(i) if text[i..].starts_with("\r\n") => "\r\n",
            _ => "\n",
        };
        Self { s: Scanner::new(text), newline }
    }
}

/// Error construction.
impl Parser<'_> {
    /// Creates an error pointing at the given byte index.
    fn error_at(&self, index: usize, message: impl Into<String>) -> ParseError {
        ParseError::new(ErrorKind::Malformed, self.s.string(), index, message)
    }

    /// Creates an error for a rule that a value starting at the byte index `start`
    /// violates.
    fn violation(&self, start: usize, violation: Violation) -> ParseError {
        self.error_at(start + violation.offset, violation.message)
    }

    /// Creates an error pointing at the cursor.
    fn error(&self, message: impl Into<String>) -> ParseError {
        self.error_at(self.s.cursor(), message)
    }

    /// Creates an error pointing at the cursor that says what was expected
    /// instead of the next character.
    fn expected(&self, what: &str) -> ParseError {
        self.error(format!("expected {what}, found {}", self.found()))
    }

    /// Describes what is at the cursor.
    fn found(&self) -> String {
        match self.s.peek() {
            None => "end of file".into(),
            Some('\r' | '\n') => "a linebreak".into(),
            Some(' ') => "a space".into(),
            Some('\t') => "a tab".into(),
            Some(c) => format!("`{c}`"),
        }
    }
}

/// Whitespace.
impl Parser<'_> {
    /// Eats a linebreak of the file's style.
    fn newline(&mut self) -> Result<()> {
        if self.s.eat_if(self.newline) {
            return Ok(());
        }

        Err(match self.s.peek() {
            Some('\r' | '\n') => {
                let style = if self.newline == "\n" { "LF" } else { "CRLF" };
                self.error(format!(
                    "inconsistent linebreak: the first linebreak of the file is {style}, \
                     so all others must be {style} too",
                ))
            }
            Some(' ' | '\t') if self.at_line_end_after_whitespace() => {
                self.error("trailing whitespace is only allowed within license texts")
            }
            _ => self.expected("a linebreak"),
        })
    }

    /// Whether only spaces and tabs are left until the end of the line.
    fn at_line_end_after_whitespace(&self) -> bool {
        let mut s = self.s;
        s.eat_while([' ', '\t']);
        s.done() || s.at(['\r', '\n'])
    }

    /// Eats a single space or linebreak that separates two words of a line that
    /// may be wrapped.
    fn space(&mut self) -> Result<()> {
        match self.s.peek() {
            Some(' ') => {
                self.s.eat();
            }
            Some('\r' | '\n') => self.newline()?,
            _ => return Err(self.expected("a space or linebreak")),
        }

        if self.s.at([' ', '\r', '\n']) {
            return Err(self.error(
                "expected a single space or linebreak, but found more whitespace; \
                 multiple spaces and empty lines are not allowed here",
            ));
        }
        Ok(())
    }

    /// Eats a line of text that must not start or end with whitespace and must
    /// not contain tabs, without the linebreak.
    fn text(&mut self) -> Result<&str> {
        let start = self.s.cursor();
        let text = self.s.eat_until(['\r', '\n']);

        if text.is_empty() {
            return Err(self.expected("text"));
        }
        if text.starts_with(' ') {
            return Err(self.error_at(start, "unexpected extra space"));
        }
        if let Some(i) = text.find('\t') {
            return Err(
                self.error_at(start + i, "tabs are only allowed within license texts")
            );
        }
        if text.ends_with(' ') {
            let end = start + text.trim_end_matches(' ').len();
            return Err(self.error_at(
                end,
                "trailing whitespace is only allowed within license texts",
            ));
        }
        Ok(text)
    }

    /// The number of spaces at the start of the next line.
    fn indent(&self) -> usize {
        self.s.after().bytes().take_while(|&b| b == b' ').count()
    }

    /// Eats exactly the given number of spaces.
    fn eat_indent(&mut self, indent: usize) -> Result<()> {
        if self.s.eat_while(' ').len() != indent {
            return Err(self.error(format!(
                "expected an indentation of {indent} spaces to align with the start of \
                 the property value",
            )));
        }
        Ok(())
    }
}

/// The file structure.
impl Parser<'_> {
    /// Eats the header, if the text starts with it, and returns whether it did.
    ///
    /// The linebreaks after the header are not eaten.
    fn header(&mut self) -> bool {
        HEADER.split(' ').enumerate().all(|(i, word)| {
            (i == 0 || self.s.eat_if(' ') || self.s.eat_if(self.newline))
                && self.s.eat_if(word)
        })
    }

    /// Eats all license blocks until the end of the file.
    fn blocks(&mut self) -> Result<Vec<LicenseBlock>> {
        let mut blocks = vec![self.block()?];
        loop {
            if self.s.done() {
                return Err(self.error(
                    "the file must end with a linebreak after the last end marker",
                ));
            }
            self.newline()?;
            if self.s.done() {
                return Ok(blocks);
            }
            if self.s.after().chars().all(|c| matches!(c, '\r' | '\n')) {
                return Err(self.error(
                    "the file must end with exactly one linebreak after the last end \
                     marker",
                ));
            }
            self.block_separator()?;
            blocks.push(self.block()?);
        }
    }

    /// Eats the two empty lines between two license blocks.
    fn block_separator(&mut self) -> Result<()> {
        for _ in 0..2 {
            if self.s.at('=') {
                return Err(self.error(
                    "license blocks must be separated by two empty lines (three \
                     linebreaks after the end marker)",
                ));
            }
            self.newline()?;
        }
        Ok(())
    }

    /// Eats a license block, up to and excluding the linebreak after its end
    /// marker.
    fn block(&mut self) -> Result<LicenseBlock> {
        let marker_len = self.start_marker()?;
        let license = self.license_marker()?;

        let mut items = vec![self.item()?];
        while self.s.at(ITEM_START) {
            // The license text cannot start with an item start, so a license text
            // that does is read as another item.
            let item = self.item().map_err(|err| {
                err.with_hint(
                    "if this is the start of the license text, note that it must not \
                     start with `* `",
                )
            })?;
            items.push(item);
        }

        let (text, marker) = self.verbatim(Verbatim::License, marker_len)?;
        let notice = if marker == '-' {
            self.newline()?;
            Some(self.verbatim(Verbatim::Notice, marker_len)?.0)
        } else {
            None
        };

        Ok(LicenseBlock { license, items, text, notice })
    }

    /// Eats a start marker and returns its length.
    fn start_marker(&mut self) -> Result<usize> {
        let start = self.s.cursor();
        let len = self.s.eat_while('=').len();
        if len == 0 {
            return Err(self.expected(&format!(
                "a start marker of at least {MIN_MARKER_LEN} `=` characters",
            )));
        }
        if len < MIN_MARKER_LEN {
            return Err(self.error_at(
                start,
                format!(
                    "the start marker has {len} `=` characters, but at least \
                     {MIN_MARKER_LEN} are required",
                ),
            ));
        }
        self.newline()?;
        Ok(len)
    }
}

/// License and notice texts.
impl Parser<'_> {
    /// Eats a verbatim text and the marker line that ends it, up to and
    /// excluding the linebreak after the marker.
    ///
    /// Returns the text and the character that the marker consists of: `=` for
    /// an end marker or `-` for a notice marker.
    fn verbatim(&mut self, kind: Verbatim, marker_len: usize) -> Result<(String, char)> {
        self.check_verbatim_start(kind)?;

        let start = self.s.cursor();
        loop {
            let line_start = self.s.cursor();
            let line = self.s.eat_until(['\r', '\n']);
            let marker = terminators(kind)
                .iter()
                .copied()
                .find(|&c| line.len() == marker_len && line.chars().all(|l| l == c));

            if let Some(marker) = marker {
                if line_start == start {
                    return Err(self.error_at(line_start, empty_message(kind)));
                }
                // The linebreak before the marker is not part of the text.
                let end = line_start - self.newline.len();
                return Ok((self.s.get(start..end).to_owned(), marker));
            }
            if self.s.done() {
                return Err(self.error(format!(
                    "missing end marker: expected a line of exactly {marker_len} `=` \
                     characters after the {}",
                    kind.name(),
                )));
            }
            self.newline()?;
        }
    }

    /// Checks that the first line of a verbatim text has non-whitespace content.
    fn check_verbatim_start(&self, kind: Verbatim) -> Result<()> {
        let mut s = self.s;
        let line = s.eat_until(['\r', '\n']);
        let Err(message) = rules::verbatim_start(kind, line) else { return Ok(()) };

        Err(if self.s.done() {
            self.expected(&format!("the {}", kind.name()))
        } else if line.is_empty() && kind == Verbatim::License {
            self.error(
                "expected an empty line between item and license text, found a second \
                 empty line; the license text must start with a non-whitespace line",
            )
        } else {
            self.error(message)
        })
    }
}

/// The license marker.
impl Parser<'_> {
    /// Eats the license marker and the empty line after it.
    fn license_marker(&mut self) -> Result<License> {
        let req = self.license_req()?;
        self.space()?;

        let name = if matches!(req.license, LicenseItem::Other(_)) {
            let name = self.license_name()?;
            self.space()?;
            Some(name)
        } else if self.s.at('(') {
            return Err(self.error(
                "only `LicenseRef-` and `DocumentRef-` identifiers take a license name",
            ));
        } else {
            None
        };

        for (i, word) in MARKER_WORDS.into_iter().enumerate() {
            if i > 0 {
                self.space()?;
            }
            if !self.s.eat_if(word) {
                return Err(self.expected(&format!("`{word}`")));
            }
        }

        self.newline()?;
        self.newline()?;
        Ok(License { req, name })
    }

    /// Eats the SPDX expression at the start of the license marker and ensures
    /// that it consists of a single license requirement.
    ///
    /// Stops before the space or linebreak that follows the expression.
    fn license_req(&mut self) -> Result<LicenseReq> {
        let mut scanned = ScannedExpression::default();
        loop {
            let start = self.s.cursor();
            let word = self.s.eat_until(char::is_whitespace);
            if word.is_empty() {
                return Err(self.expected("an SPDX license identifier"));
            }
            scanned.push(word, start);

            let end = self.s.cursor();
            if self.s.at([' ', '\r', '\n']) {
                self.space()?;
                if self.expression_continues(word) {
                    continue;
                }
            }
            self.s.jump(end);
            break;
        }

        rules::license_req(&scanned.text)
            .map_err(|v| self.error_at(scanned.source_index(v.offset), v.message))
    }

    /// Whether the next word still belongs to the SPDX expression, given the
    /// word before it.
    ///
    /// The expression ends before the license name in parentheses and before the
    /// fixed part of the marker (which starts with dashes, unlike any
    /// identifier). After an operator, a parenthesis is not a name but part of a
    /// compound expression, which is reported as such.
    fn expression_continues(&self, last: &str) -> bool {
        let mut s = self.s;
        match s.eat_until(char::is_whitespace) {
            "" => false,
            next if next.starts_with('-') => false,
            next if next.starts_with('(') => matches!(last, "AND" | "OR" | "WITH"),
            _ => true,
        }
    }

    /// Eats a license name in parentheses.
    fn license_name(&mut self) -> Result<String> {
        let open = self.s.cursor();
        if !self.s.eat_if('(') {
            return Err(self.expected(
                "the license name in parentheses, which license references require",
            ));
        }

        let mut name = String::new();
        let mut depth = 1;
        while depth > 0 {
            match self.s.peek() {
                None => {
                    return Err(self
                        .error_at(open, "unclosed parenthesis around the license name"));
                }
                Some('\t') => {
                    return Err(self.error("tabs are not allowed in the license name"));
                }
                Some(' ' | '\r' | '\n') => {
                    if name.is_empty() {
                        return Err(
                            self.error("the license name must not start with a space")
                        );
                    }
                    self.space()?;
                    if self.s.at(')') {
                        return Err(
                            self.error("the license name must not end with a space")
                        );
                    }
                    name.push(' ');
                }
                Some(c) => {
                    self.s.eat();
                    match c {
                        '(' => depth += 1,
                        ')' => depth -= 1,
                        _ => {}
                    }
                    if depth > 0 {
                        name.push(c);
                    }
                }
            }
        }

        if name.is_empty() {
            return Err(self.error_at(open, "the license name must not be empty"));
        }
        Ok(name)
    }
}

/// An SPDX expression that was scanned from a possibly wrapped marker.
///
/// Linebreaks between words are replaced by spaces in the text, so we remember
/// where each word came from to point errors to the right place.
#[derive(Default)]
struct ScannedExpression {
    /// The expression with single spaces between its words.
    text: String,
    /// For each word, its byte offset in `text` and in the source file.
    words: Vec<(usize, usize)>,
}

impl ScannedExpression {
    /// Appends a word that starts at the given byte index in the source file.
    fn push(&mut self, word: &str, source_index: usize) {
        if !self.text.is_empty() {
            self.text.push(' ');
        }
        self.words.push((self.text.len(), source_index));
        self.text.push_str(word);
    }

    /// Maps a byte offset in the expression text to a byte index in the source
    /// file.
    fn source_index(&self, offset: usize) -> usize {
        let i = self.words.partition_point(|&(start, _)| start <= offset);
        let (start, source_index) = self.words[i.saturating_sub(1)];
        source_index + offset - start
    }
}

/// Items and their properties.
impl Parser<'_> {
    /// Eats an item including the empty line after its property list.
    fn item(&mut self) -> Result<Item> {
        if !self.s.eat_if(ITEM_START) {
            return Err(self.expected("an item starting with `* `"));
        }
        let (description, lookalike) = self.description()?;
        let list_start = self.s.cursor();
        self.property_list(description).map_err(|err| {
            if lookalike && err.index == list_start {
                err.with_hint(
                    "a line of the description starts like a property, so the empty \
                     line between the description and the property list may be missing",
                )
            } else {
                err
            }
        })
    }

    /// Eats the description of an item and the empty line after it.
    ///
    /// Also returns whether a continuation line starts like a property, which is
    /// valid but suggests that the empty line before the property list is missing.
    fn description(&mut self) -> Result<(String, bool)> {
        let mut description = self.text()?.to_owned();
        let mut lookalike = false;
        self.newline()?;

        while !self.s.at(self.newline) {
            if !self.s.eat_if("  ") {
                return Err(self.expected(
                    "an empty line or a continuation line starting with two spaces",
                ));
            }
            lookalike |= PROPERTY_STARTS.iter().any(|start| self.s.at(*start));
            description.push(' ');
            description.push_str(self.text()?);
            self.newline()?;
        }

        self.newline()?;
        Ok((description, lookalike))
    }

    /// Eats the property list of an item and the empty line after it.
    fn property_list(&mut self, description: String) -> Result<Item> {
        let list_start = self.s.cursor();
        let mut package = None;
        let mut paths = None;
        let mut source = None;
        let mut notice = None;
        let mut also_see = None;
        let mut inclusion = Inclusion::Always;
        let mut last_key: Option<Key> = None;

        loop {
            let line_start = self.s.cursor();
            self.property_indent()?;

            if let Some(statement) = self.inclusion() {
                inclusion = statement;
                self.newline()?;
                break;
            }

            let key_start = self.s.cursor();
            let key = self.key()?;
            if let Some(last) = last_key.filter(|last| key.order() <= last.order()) {
                return Err(self.error_at(
                    key_start,
                    if key.order() == last.order() {
                        format!("duplicate property `{}`", key.name())
                    } else {
                        format!(
                            "property `{}` is out of order; properties must appear \
                             as `Package`, `Path` or `Paths`, `Source`, `Notice`, \
                             `Also see`",
                            key.name(),
                        )
                    },
                ));
            }
            last_key = Some(key);

            // Wrapped lines are indented to align with the start of the value.
            let indent = self.s.cursor() - line_start;
            match key {
                Key::Package => package = Some(self.package(indent)?),
                Key::Path | Key::Paths => paths = Some(self.paths(key, indent)?),
                Key::Source => source = Some(self.source()?),
                Key::Notice => notice = Some(self.wrapped_text(indent)?),
                Key::AlsoSee => also_see = Some(self.wrapped_text(indent)?),
            }

            if self.s.at(self.newline) {
                break;
            }
        }

        if !self.s.at(self.newline) {
            return Err(self.expected("an empty line after the inclusion statement"));
        }
        if package.is_none() && paths.is_none() {
            return Err(self.error_at(
                list_start,
                "missing required property: at least one of `Package` and `Path` or \
                 `Paths` is required",
            ));
        }
        let source = source.ok_or_else(|| {
            self.error_at(list_start, "missing required property `Source`")
        })?;
        self.newline()?;

        Ok(Item {
            description,
            package,
            paths: paths.unwrap_or_default(),
            source,
            notice,
            also_see,
            inclusion,
        })
    }

    /// Eats the two spaces that indent a property.
    fn property_indent(&mut self) -> Result<()> {
        if !self.s.eat_if("  ") {
            return Err(
                self.expected("a property indented with two spaces or an empty line")
            );
        }
        if self.s.at(' ') {
            return Err(self.error("properties must be indented with exactly two spaces"));
        }
        Ok(())
    }

    /// Eats an inclusion statement, if there is one.
    fn inclusion(&mut self) -> Option<Inclusion> {
        let mut s = self.s;
        let line = s.eat_until(['\r', '\n']);
        let (_, inclusion) = INCLUSIONS.into_iter().find(|&(text, _)| text == line)?;
        self.s = s;
        Some(inclusion)
    }

    /// Eats a property key and the colon and space after it.
    fn key(&mut self) -> Result<Key> {
        let start = self.s.cursor();
        let name = self.s.eat_until([':', '\r', '\n']);
        if !self.s.eat_if(": ") {
            self.s.jump(start);
            return Err(self.error(
                "expected a property (`Key: value`) or an inclusion statement (`Not \
                 included when used as a library`, `... as a binary`, or `... as a \
                 binary or a library`)",
            ));
        }

        Key::from_name(name).ok_or_else(|| {
            self.error_at(
                start,
                format!(
                    "unknown property `{name}`; expected `Package`, `Path`, `Paths`, \
                     `Source`, `Notice`, or `Also see`",
                ),
            )
        })
    }

    /// Eats the value of a `Package` property and its linebreak.
    ///
    /// The components can be separated by a linebreak instead of a space.
    fn package(&mut self, indent: usize) -> Result<Package> {
        let name = self.package_name()?;
        self.property_space(indent, "the package version")?;
        let version = self.package_version()?;
        self.property_space(indent, "the package ecosystem in parentheses")?;
        let ecosystem = self.ecosystem()?;
        self.newline()?;
        Ok(Package { name, version, ecosystem })
    }

    /// Eats a single space or a linebreak and the indentation that aligns the
    /// next line with the start of the property value. The value is incomplete
    /// without the `next` component.
    fn property_space(&mut self, indent: usize, next: &str) -> Result<()> {
        if self.s.at(['\r', '\n']) {
            // A line that is not indented deeper than a property cannot continue
            // the value.
            if self
                .s
                .after()
                .strip_prefix(self.newline)
                .is_some_and(|rest| rest.bytes().take_while(|&b| b == b' ').count() <= 2)
            {
                return Err(self.expected(next));
            }
            self.newline()?;
            return self.eat_indent(indent);
        }

        if !self.s.eat_if(' ') {
            return Err(self.expected("a space or linebreak"));
        }
        if self.s.at([' ', '\r', '\n']) {
            return Err(self.error(
                "expected a single space or linebreak, but found more whitespace",
            ));
        }
        Ok(())
    }

    /// Eats a package name enclosed in backticks.
    fn package_name(&mut self) -> Result<String> {
        if !self.s.eat_if('`') {
            return Err(self.expected("the package name enclosed in backticks"));
        }

        let start = self.s.cursor();
        let name = self.s.eat_until(['`', '\r', '\n']);
        if !self.s.eat_if('`') {
            return Err(
                self.expected("a closing backtick; the package name must not wrap")
            );
        }

        rules::package_name(name).map_err(|v| self.violation(start, v))?;
        if !self.s.done() && !self.s.at(char::is_whitespace) {
            return Err(self.error(
                "expected a space or linebreak after the closing backtick; the package \
                 name must not contain backticks",
            ));
        }
        Ok(name.to_owned())
    }

    /// Eats a package version.
    fn package_version(&mut self) -> Result<String> {
        let start = self.s.cursor();
        let version = self.s.eat_until(char::is_whitespace);
        rules::package_version(version).map_err(|v| self.violation(start, v))?;
        Ok(version.to_owned())
    }

    /// Eats a package ecosystem in parentheses.
    fn ecosystem(&mut self) -> Result<String> {
        if !self.s.eat_if('(') {
            return Err(self.expected("the package ecosystem in parentheses"));
        }

        let start = self.s.cursor();
        let ecosystem = self.s.eat_until([')', '\r', '\n']);
        if !self.s.eat_if(')') {
            return Err(self.expected("a closing parenthesis"));
        }

        rules::ecosystem(ecosystem).map_err(|v| self.violation(start, v))?;
        Ok(ecosystem.to_owned())
    }

    /// Eats the value of a `Path` or `Paths` property and its linebreak.
    ///
    /// Paths can only wrap after a comma.
    fn paths(&mut self, key: Key, indent: usize) -> Result<Vec<String>> {
        let mut paths = vec![self.path()?];
        while self.s.at(',') {
            if key == Key::Path {
                return Err(
                    self.error("`Path` takes exactly one path; use `Paths` for a list")
                );
            }
            self.s.eat();

            if self.s.at(['\r', '\n']) {
                self.newline()?;
                self.eat_indent(indent)?;
            } else if !self.s.eat_if(' ') {
                return Err(self.expected("a space or linebreak after the comma"));
            }
            paths.push(self.path()?);
        }
        self.newline()?;
        Ok(paths)
    }

    /// Eats a path enclosed in backticks.
    fn path(&mut self) -> Result<String> {
        if !self.s.eat_if('`') {
            return Err(self.expected("a path enclosed in backticks"));
        }

        let start = self.s.cursor();
        let path = self.s.eat_until(['`', '\r', '\n']);
        if !self.s.eat_if('`') {
            return Err(self.expected(
                "a closing backtick; paths must not wrap, except after a comma",
            ));
        }

        rules::path(path).map_err(|v| self.violation(start, v))?;
        Ok(path.to_owned())
    }

    /// Eats the value of a `Source` property and its linebreak.
    fn source(&mut self) -> Result<String> {
        let start = self.s.cursor();
        let uri = self.text()?.to_owned();
        rules::source(&uri).map_err(|v| self.violation(start, v))?;

        self.newline()?;
        if self.indent() > 2 {
            return Err(self.error("a URI must not wrap onto the next line"));
        }
        Ok(uri)
    }

    /// Eats the value of a property that can wrap anywhere, up to and including
    /// its last linebreak. Wraps count as single spaces.
    fn wrapped_text(&mut self, indent: usize) -> Result<String> {
        let mut value = self.text()?.to_owned();
        self.newline()?;

        // A line indented deeper than a property continues the value.
        while self.indent() > 2 {
            self.eat_indent(indent)?;
            value.push(' ');
            value.push_str(self.text()?);
            self.newline()?;
        }
        Ok(value)
    }
}

/// The keys of an item's property list.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Key {
    Package,
    Path,
    Paths,
    Source,
    Notice,
    AlsoSee,
}

impl Key {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "Package" => Self::Package,
            "Path" => Self::Path,
            "Paths" => Self::Paths,
            "Source" => Self::Source,
            "Notice" => Self::Notice,
            "Also see" => Self::AlsoSee,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Self::Package => "Package",
            Self::Path => "Path",
            Self::Paths => "Paths",
            Self::Source => "Source",
            Self::Notice => "Notice",
            Self::AlsoSee => "Also see",
        }
    }

    /// The position of the key in the required order of properties.
    fn order(self) -> u8 {
        match self {
            Self::Package => 0,
            Self::Path | Self::Paths => 1,
            Self::Source => 2,
            Self::Notice => 3,
            Self::AlsoSee => 4,
        }
    }
}

/// The characters of which a line that ends the text can consist.
///
/// The license text ends at a notice marker or an end marker, the notice text
/// only at an end marker.
fn terminators(kind: Verbatim) -> &'static [char] {
    match kind {
        Verbatim::License => &['-', '='],
        Verbatim::Notice => &['='],
    }
}

/// The error message for a text that is directly followed by a marker.
fn empty_message(kind: Verbatim) -> &'static str {
    match kind {
        Verbatim::License => "the license text is empty",
        Verbatim::Notice => {
            "the notice text is empty; omit the notice marker if there is no notice"
        }
    }
}
