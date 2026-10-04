//! A small backtracking matcher for the basic regular expressions `-bp` takes.
//!
//! The manual asks for a BRE, so the syntax is the POSIX one: `.`, `[...]`,
//! `*`, `^`, `$`, `\(...\)` and `\|`, plus the `\?`, `\+` and `\{m,n\}` interval
//! operators and the GNU additions `\w`, `\W`, `\b`, `\B`, `\<` and `\>`.
//! Matching is over bytes, which is what the rest of the tools see.

/// A compiled pattern.
#[derive(Clone, Debug)]
pub struct Regex {
    root: Node,
    source: String,
}

/// Two patterns are the same when their text is, which is all a STYLE compares.
impl PartialEq for Regex {
    fn eq(&self, other: &Regex) -> bool {
        self.source == other.source
    }
}

impl Eq for Regex {}

/// One node of the syntax tree.
#[derive(Clone, Debug)]
enum Node {
    Empty,
    Literal(u8),
    Any,
    Class(Box<ClassSet>),
    Start,
    End,
    WordBoundary(bool),
    WordStart,
    WordEnd,
    Concat(Vec<Node>),
    Alternate(Vec<Node>),
    Repeat(Box<Node>, usize, Option<usize>),
}

/// The contents of a bracket expression.
#[derive(Clone, Debug, Default)]
struct ClassSet {
    negated: bool,
    ranges: Vec<(u8, u8)>,
    named: Vec<(ClassName, bool)>,
}

/// The character classes the manual's POSIX spells `[:alpha:]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClassName {
    Alpha,
    Digit,
    Alnum,
    Upper,
    Lower,
    Space,
    Blank,
    Punct,
    Print,
    Graph,
    Cntrl,
    Xdigit,
    Word,
}

impl ClassName {
    fn parse(name: &str) -> Option<ClassName> {
        Some(match name {
            "alpha" => ClassName::Alpha,
            "digit" => ClassName::Digit,
            "alnum" => ClassName::Alnum,
            "upper" => ClassName::Upper,
            "lower" => ClassName::Lower,
            "space" => ClassName::Space,
            "blank" => ClassName::Blank,
            "punct" => ClassName::Punct,
            "print" => ClassName::Print,
            "graph" => ClassName::Graph,
            "cntrl" => ClassName::Cntrl,
            "xdigit" => ClassName::Xdigit,
            "word" => ClassName::Word,
            _ => return None,
        })
    }

    fn contains(self, byte: u8) -> bool {
        match self {
            ClassName::Alpha => byte.is_ascii_alphabetic(),
            ClassName::Digit => byte.is_ascii_digit(),
            ClassName::Alnum => byte.is_ascii_alphanumeric(),
            ClassName::Upper => byte.is_ascii_uppercase(),
            ClassName::Lower => byte.is_ascii_lowercase(),
            ClassName::Space => byte.is_ascii_whitespace(),
            ClassName::Blank => byte == b' ' || byte == b'\t',
            ClassName::Punct => byte.is_ascii_punctuation(),
            ClassName::Print => (0x20..0x7f).contains(&byte),
            ClassName::Graph => (0x21..0x7f).contains(&byte),
            ClassName::Cntrl => byte < 0x20 || byte == 0x7f,
            ClassName::Xdigit => byte.is_ascii_hexdigit(),
            ClassName::Word => byte.is_ascii_alphanumeric() || byte == b'_',
        }
    }
}

impl ClassSet {
    fn contains(&self, byte: u8) -> bool {
        let mut found = self.ranges.iter().any(|(low, high)| *low <= byte && byte <= *high);
        if !found {
            found = self
                .named
                .iter()
                .any(|(name, negated)| name.contains(byte) != *negated);
        }
        found != self.negated
    }
}

/// A parse failure; the caller turns it into a diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub offset: usize,
}

/// A pattern being read, so the parser can look ahead.
struct Parser<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    fn peek_at(&self, ahead: usize) -> Option<u8> {
        self.bytes.get(self.position + ahead).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let byte = self.peek();
        if byte.is_some() {
            self.position += 1;
        }
        byte
    }

    /// The closing brace of an interval, which a BRE writes escaped.
    fn close_brace(&mut self) -> bool {
        if self.peek() == Some(b'\\') {
            self.position += 1;
        }
        if self.peek() == Some(b'}') {
            self.position += 1;
            true
        } else {
            false
        }
    }

    /// Read the digits of an interval, given that `\{` was already seen.
    fn interval(&mut self) -> Result<(usize, Option<usize>), Error> {
        let low = self.digits();
        if self.peek() == Some(b',') {
            self.position += 1;
            if self.close_brace() {
                return Ok((low, None));
            }
            let high = self.digits();
            if self.close_brace() {
                return Ok((low, Some(high)));
            }
        } else if self.close_brace() {
            return Ok((low, Some(low)));
        }
        Err(Error {
            offset: self.position,
        })
    }

    fn digits(&mut self) -> usize {
        let mut value = 0usize;
        while let Some(byte) = self.peek() {
            if byte.is_ascii_digit() {
                value = value * 10 + usize::from(byte - b'0');
                self.position += 1;
            } else {
                break;
            }
        }
        value
    }

    /// A bracket expression, with `[` already consumed.
    fn bracket(&mut self) -> Result<ClassSet, Error> {
        let mut set = ClassSet::default();
        if self.peek() == Some(b'^') {
            set.negated = true;
            self.position += 1;
        }
        // A `]` in the first position is a literal.
        let mut first = true;
        loop {
            let byte = match self.next() {
                Some(byte) => byte,
                None => {
                    return Err(Error {
                        offset: self.position,
                    })
                }
            };
            if byte == b']' && !first {
                return Ok(set);
            }
            first = false;
            // [:alpha:] and friends.
            if byte == b'[' && self.peek() == Some(b':') {
                let start = self.position;
                self.position += 1;
                let mut name = Vec::new();
                while let Some(inner) = self.peek() {
                    self.position += 1;
                    if inner == b':' {
                        break;
                    }
                    name.push(inner);
                }
                if self.peek() == Some(b']') {
                    self.position += 1;
                    match ClassName::parse(&String::from_utf8_lossy(&name)) {
                        Some(found) => {
                            set.named.push((found, false));
                            continue;
                        }
                        None => {
                            set.named.push((ClassName::Punct, false));
                            let _ = start;
                            set.ranges.extend(name.windows(2).filter_map(|pair| {
                                if pair[0] == b'^' {
                                    Some((pair[1], u8::MAX))
                                } else {
                                    None
                                }
                            }));
                            continue;
                        }
                    }
                }
                // Not a character class after all: treat the '[' literally.
                set.ranges.push((b'[', b'['));
                set.ranges.push((b':', b':'));
                continue;
            }
            let low = if byte == b'\\' {
                match self.next() {
                    Some(escaped) => escaped,
                    None => {
                        return Err(Error {
                            offset: self.position,
                        })
                    }
                }
            } else {
                byte
            };
            // A range, unless the '-' is the last character in the set.
            if self.peek() == Some(b'-')
                && self.peek_at(1).is_some()
                && self.peek_at(1) != Some(b']')
            {
                self.position += 1;
                let high = match self.next() {
                    Some(high) => high,
                    None => {
                        return Err(Error {
                            offset: self.position,
                        })
                    }
                };
                let high = if high == b'\\' { self.next().unwrap_or(high) } else { high };
                set.ranges.push((low, high));
            } else {
                set.ranges.push((low, low));
            }
        }
    }

    /// One atom, without its postfix operators.
    fn atom(&mut self) -> Result<Node, Error> {
        match self.next() {
            None => Ok(Node::Empty),
            Some(b'.') => Ok(Node::Any),
            Some(b'^') => Ok(Node::Start),
            Some(b'$') => Ok(Node::End),
            Some(b'[') => Ok(Node::Class(Box::new(self.bracket()?))),
            // A plain '(' is literal in a BRE; a group is spelled '\('.
            Some(b'(') => Ok(Node::Literal(b'(')),
            Some(b'\\') => match self.next() {
                None => Ok(Node::Literal(b'\\')),
                Some(b'(') => {
                    let node = self.alternation()?;
                    if !(self.peek() == Some(b'\\') && self.peek_at(1) == Some(b')')) {
                        return Err(Error {
                            offset: self.position,
                        });
                    }
                    self.position += 2;
                    Ok(node)
                }
                Some(b'w') => Ok(Node::Class(Box::new(word_class()))),
                Some(b'W') => Ok(Node::Class(Box::new({
                    let mut set = word_class();
                    set.negated = !set.negated;
                    set
                }))),
                Some(b'b') => Ok(Node::WordBoundary(true)),
                Some(b'B') => Ok(Node::WordBoundary(false)),
                Some(b'<') => Ok(Node::WordStart),
                Some(b'>') => Ok(Node::WordEnd),
                Some(other) => Ok(Node::Literal(other)),
            },
            Some(byte) => Ok(Node::Literal(byte)),
        }
    }

    /// The postfix operators, applied to `atom`.
    fn postfix(&mut self, atom: Node) -> Result<Node, Error> {
        let mut node = atom;
        loop {
            match self.peek() {
                Some(b'*') => {
                    self.position += 1;
                    node = Node::Repeat(Box::new(node), 0, None);
                }
                Some(b'\\') => match self.peek_at(1) {
                    Some(b'?') => {
                        self.position += 2;
                        node = Node::Repeat(Box::new(node), 0, Some(1));
                    }
                    Some(b'+') => {
                        self.position += 2;
                        node = Node::Repeat(Box::new(node), 1, None);
                    }
                    Some(b'{') => {
                        let save = self.position;
                        self.position += 2;
                        match self.interval() {
                            Ok((low, high)) => {
                                node = Node::Repeat(Box::new(node), low, high);
                            }
                            Err(_) => {
                                // GNU also allows a literal brace, so a bad
                                // interval is only a brace.
                                self.position = save;
                                return Ok(node);
                            }
                        }
                    }
                    _ => return Ok(node),
                },
                _ => return Ok(node),
            }
        }
    }

    fn concat(&mut self) -> Result<Node, Error> {
        let mut parts = Vec::new();
        while !self.at_branch_end() {
            let atom = self.atom()?;
            let atom = match atom {
                Node::Empty => break,
                other => other,
            };
            parts.push(self.postfix(atom)?);
        }
        Ok(match parts.len() {
            0 => Node::Empty,
            1 => parts.pop().expect("one part"),
            _ => Node::Concat(parts),
        })
    }

    /// True at a `|` or `\|`, or at the `\)` that closes the enclosing group.
    fn at_branch_end(&self) -> bool {
        match self.peek() {
            None => true,
            Some(b'|') => true,
            Some(b'\\') => matches!(self.peek_at(1), Some(b'|') | Some(b')')),
            _ => false,
        }
    }

    fn alternation(&mut self) -> Result<Node, Error> {
        let mut branches = vec![self.concat()?];
        loop {
            match self.peek() {
                Some(b'|') => self.position += 1,
                Some(b'\\') if self.peek_at(1) == Some(b'|') => self.position += 2,
                _ => break,
            }
            branches.push(self.concat()?);
        }
        Ok(match branches.len() {
            1 => branches.pop().expect("one branch"),
            _ => Node::Alternate(branches),
        })
    }
}

fn word_class() -> ClassSet {
    ClassSet {
        negated: false,
        ranges: Vec::new(),
        named: vec![(ClassName::Word, false)],
    }
}

impl Regex {
    /// Compile `pattern` as a BRE.
    pub fn new(pattern: &str) -> Result<Regex, Error> {
        let mut parser = Parser {
            bytes: pattern.as_bytes(),
            position: 0,
        };
        let root = parser.alternation()?;
        Ok(Regex {
            root,
            source: pattern.to_string(),
        })
    }

    /// Whether the pattern matches anywhere in `text`.
    pub fn is_match(&self, text: &[u8]) -> bool {
        for start in 0..=text.len() {
            if matches_node(&self.root, text, start, &mut |_| true) {
                return true;
            }
        }
        false
    }
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Match `node` at `position`, handing the rest to `next`.
fn matches_node(
    node: &Node,
    text: &[u8],
    position: usize,
    next: &mut dyn FnMut(usize) -> bool,
) -> bool {
    match node {
        Node::Empty => next(position),
        Node::Literal(byte) => {
            text.get(position) == Some(byte) && next(position + 1)
        }
        Node::Any => match text.get(position) {
            Some(byte) if *byte != b'\n' => next(position + 1),
            _ => false,
        },
        Node::Class(set) => match text.get(position) {
            Some(byte) if set.contains(*byte) => next(position + 1),
            _ => false,
        },
        Node::Start => position == 0 && next(position),
        Node::End => position == text.len() && next(position),
        Node::WordBoundary(wanted) => {
            let before = position > 0 && is_word_byte(text[position - 1]);
            let after = position < text.len() && is_word_byte(text[position]);
            (before != after) == *wanted && next(position)
        }
        Node::WordStart => {
            let before = position > 0 && is_word_byte(text[position - 1]);
            let after = position < text.len() && is_word_byte(text[position]);
            !before && after && next(position)
        }
        Node::WordEnd => {
            let before = position > 0 && is_word_byte(text[position - 1]);
            let after = position < text.len() && is_word_byte(text[position]);
            before && !after && next(position)
        }
        Node::Concat(parts) => match parts.split_first() {
            None => next(position),
            Some((first, rest)) => matches_node(first, text, position, &mut |after_first| {
                matches_sequence(rest, text, after_first, next)
            }),
        },
        Node::Alternate(branches) => branches
            .iter()
            .any(|branch| matches_node(branch, text, position, next)),
        Node::Repeat(inner, low, high) => {
            matches_repeat(inner, text, position, *low, *high, next)
        }
    }
}

fn matches_sequence(
    parts: &[Node],
    text: &[u8],
    position: usize,
    next: &mut dyn FnMut(usize) -> bool,
) -> bool {
    match parts.split_first() {
        None => next(position),
        Some((first, rest)) => matches_node(first, text, position, &mut |after_first| {
            matches_sequence(rest, text, after_first, next)
        }),
    }
}

/// The greedy repetition of `inner`, backtracking on the way out.
fn matches_repeat(
    inner: &Node,
    text: &[u8],
    position: usize,
    low: usize,
    high: Option<usize>,
    next: &mut dyn FnMut(usize) -> bool,
) -> bool {
    // Try the longest run first, which is what a greedy operator does.
    fn walk(
        inner: &Node,
        text: &[u8],
        position: usize,
        low: usize,
        high: Option<usize>,
        count: usize,
        next: &mut dyn FnMut(usize) -> bool,
    ) -> bool {
        let may_take = match high {
            Some(limit) => count < limit,
            None => true,
        };
        if may_take
            && matches_node(inner, text, position, &mut |after| {
                // A repetition that consumed nothing would loop forever.
                after != position && walk(inner, text, after, low, high, count + 1, next)
            })
        {
            return true;
        }
        count >= low && next(position)
    }
    walk(inner, text, position, low, high, 0, next)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(pattern: &str, text: &str) -> bool {
        Regex::new(pattern).expect("pattern").is_match(text.as_bytes())
    }

    #[test]
    fn literals_and_stars() {
        assert!(matches("abc", "xxabcyy"));
        assert!(matches("a.c", "abc"));
        assert!(matches("a*", "bbb"));
        assert!(matches("a*b", "b"));
        assert!(matches("ab*c", "ac"));
        assert!(!matches("^abc", "xabc"));
        assert!(matches("^abc", "abcx"));
        assert!(matches("abc$", "xabc"));
        assert!(!matches("abc$", "abcx"));
    }

    #[test]
    fn brackets() {
        assert!(matches("[abc]", "b"));
        assert!(!matches("[^abc]", "b"));
        assert!(matches("[^abc]", "d"));
        assert!(matches("[a-z]", "q"));
        assert!(!matches("[a-z]", "Q"));
        assert!(matches("[]a]", "]"));
        assert!(matches("[[:digit:]]", "5"));
        assert!(!matches("[[:digit:]]", "x"));
        assert!(matches("[a-]", "-"));
    }

    #[test]
    fn intervals_and_optionals() {
        assert!(matches(r"a\{2\}", "aa"));
        assert!(!matches(r"a\{2\}", "a"));
        assert!(matches(r"a\{2,\}", "aaa"));
        assert!(matches(r"^a\{0,2\}$", ""));
        assert!(matches(r"^a\{0,2\}$", "aa"));
        assert!(!matches(r"^a\{0,2\}$", "aaa"));
        assert!(matches(r"^ab\?$", "a"));
        assert!(matches(r"ab\+", "ab"));
        assert!(!matches(r"^ab\?$", "ac"));
    }

    #[test]
    fn groups_and_alternation() {
        assert!(matches(r"\(ab\)*c", "ababc"));
        assert!(matches(r"cat\|dog", "dog"));
        assert!(matches(r"\(cat\|dog\)s", "dogs"));
        // A plain bracket is literal in a BRE.
        assert!(matches("(a)", "(a)"));
    }

    #[test]
    fn the_gnu_additions() {
        assert!(matches(r"\w", "a"));
        assert!(!matches(r"\w", "-"));
        assert!(matches(r"\W", "-"));
        assert!(matches(r"\<word\>", "a word here"));
        assert!(matches(r"\bword\b", "a word here"));
    }

    #[test]
    fn an_unterminated_bracket_is_an_error() {
        assert!(Regex::new("[abc").is_err());
    }
}