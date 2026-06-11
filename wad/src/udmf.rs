//! UDMF `TEXTMAP` parser (spec v1.1 grammar + the ZDoom plane-equation
//! sector fields). Unknown identifiers are ignored per the spec; fields the
//! spec marks "no valid default" are required.

use std::collections::HashMap;
use std::fmt;

/// Default sector light level per the UDMF spec.
const DEFAULT_LIGHT: i32 = 160;
/// Texture name meaning "none".
const NO_TEXTURE: &str = "-";

#[derive(Debug, PartialEq)]
pub struct UdmfError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for UdmfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TEXTMAP line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for UdmfError {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UdmfVertex {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UdmfLineDef {
    pub v1: usize,
    pub v2: usize,
    pub sidefront: usize,
    pub sideback: Option<usize>,
    pub special: i32,
    pub args: [i32; 5],
    pub id: i32,
    pub blocking: bool,
    pub blockmonsters: bool,
    pub twosided: bool,
    pub dontpegtop: bool,
    pub dontpegbottom: bool,
    pub secret: bool,
    pub blocksound: bool,
    pub dontdraw: bool,
    pub mapped: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UdmfSideDef {
    pub offsetx: i32,
    pub offsety: i32,
    /// `None` = the spec's "-" placeholder.
    pub texturetop: Option<String>,
    pub texturebottom: Option<String>,
    pub texturemiddle: Option<String>,
    pub sector: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UdmfSector {
    pub heightfloor: i32,
    pub heightceiling: i32,
    pub texturefloor: String,
    pub textureceiling: String,
    pub lightlevel: i32,
    pub special: i32,
    pub id: i32,
    /// ZDoom `floorplane_a..d`: `a*x + b*y + c*z + d = 0`, normal up.
    /// Present only when all four fields are given.
    pub floor_plane: Option<[f64; 4]>,
    /// ZDoom `ceilingplane_a..d`, normal down.
    pub ceiling_plane: Option<[f64; 4]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UdmfThing {
    pub x: f64,
    pub y: f64,
    pub height: f64,
    pub angle: i32,
    pub kind: i32,
    pub ambush: bool,
    pub skill1: bool,
    pub skill2: bool,
    pub skill3: bool,
    pub skill4: bool,
    pub skill5: bool,
    pub single: bool,
    pub dm: bool,
    pub coop: bool,
}

#[derive(Debug, Default, PartialEq)]
pub struct UdmfMap {
    pub namespace: String,
    pub vertices: Vec<UdmfVertex>,
    pub linedefs: Vec<UdmfLineDef>,
    pub sidedefs: Vec<UdmfSideDef>,
    pub sectors: Vec<UdmfSector>,
    pub things: Vec<UdmfThing>,
}

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
}

impl Value {
    fn as_f64(&self) -> Option<f64> {
        match *self {
            Self::Int(i) => Some(i as f64),
            Self::Float(f) => Some(f),
            _ => None,
        }
    }

    fn as_i32(&self) -> Option<i32> {
        match *self {
            Self::Int(i) => i32::try_from(i).ok(),
            _ => None,
        }
    }

    fn as_bool(&self) -> Option<bool> {
        match *self {
            Self::Bool(b) => Some(b),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }
}

/// One parsed block: lowercased field names to values.
struct Block {
    line: usize,
    fields: HashMap<String, Value>,
}

impl Block {
    fn err(&self, message: impl Into<String>) -> UdmfError {
        UdmfError {
            line: self.line,
            message: message.into(),
        }
    }

    fn f64_req(&self, key: &str) -> Result<f64, UdmfError> {
        self.fields
            .get(key)
            .and_then(Value::as_f64)
            .ok_or_else(|| self.err(format!("missing required float `{key}`")))
    }

    fn usize_req(&self, key: &str) -> Result<usize, UdmfError> {
        self.fields
            .get(key)
            .and_then(Value::as_i32)
            .and_then(|v| usize::try_from(v).ok())
            .ok_or_else(|| self.err(format!("missing required index `{key}`")))
    }

    fn str_req(&self, key: &str) -> Result<String, UdmfError> {
        self.fields
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| self.err(format!("missing required string `{key}`")))
    }

    fn i32_or(&self, key: &str, default: i32) -> i32 {
        self.fields
            .get(key)
            .and_then(Value::as_i32)
            .unwrap_or(default)
    }

    fn f64_or(&self, key: &str, default: f64) -> f64 {
        self.fields
            .get(key)
            .and_then(Value::as_f64)
            .unwrap_or(default)
    }

    fn bool_or(&self, key: &str) -> bool {
        self.fields
            .get(key)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    fn texture(&self, key: &str) -> Option<String> {
        let tex = self.fields.get(key).and_then(Value::as_str)?;
        (tex != NO_TEXTURE).then(|| tex.to_owned())
    }

    /// All four `<prefix>_a..d` fields, or `None` if any is absent.
    fn plane(&self, prefix: &str) -> Option<[f64; 4]> {
        let mut plane = [0.0; 4];
        for (i, suffix) in ["a", "b", "c", "d"].iter().enumerate() {
            plane[i] = self
                .fields
                .get(&format!("{prefix}_{suffix}"))
                .and_then(Value::as_f64)?;
        }
        Some(plane)
    }
}

/// Parse a `TEXTMAP` lump body.
pub fn parse_textmap(text: &str) -> Result<UdmfMap, UdmfError> {
    let mut map = UdmfMap::default();
    let mut lexer = Lexer::new(text);

    while let Some(token) = lexer.next_token()? {
        if token == Token::Semi {
            continue; // nil assignment_expr
        }
        let Token::Ident(name) = token else {
            return Err(lexer.err("expected identifier at top level"));
        };

        match lexer.peek()? {
            Some(Token::Eq) => {
                lexer.next_token()?;
                let value = lexer.value()?;
                lexer.expect(Token::Semi)?;
                if name == "namespace" {
                    map.namespace = value
                        .as_str()
                        .ok_or_else(|| lexer.err("namespace must be a string"))?
                        .to_ascii_lowercase();
                }
            }
            Some(Token::LBrace) => {
                let block = lexer.block(&name)?;
                match name.as_str() {
                    "vertex" => map.vertices.push(UdmfVertex {
                        x: block.f64_req("x")?,
                        y: block.f64_req("y")?,
                    }),
                    "linedef" => map.linedefs.push(UdmfLineDef {
                        v1: block.usize_req("v1")?,
                        v2: block.usize_req("v2")?,
                        sidefront: block.usize_req("sidefront")?,
                        sideback: match block.i32_or("sideback", -1) {
                            -1 => None,
                            sb => Some(
                                usize::try_from(sb).map_err(|_| block.err("negative sideback"))?,
                            ),
                        },
                        special: block.i32_or("special", 0),
                        args: [
                            block.i32_or("arg0", 0),
                            block.i32_or("arg1", 0),
                            block.i32_or("arg2", 0),
                            block.i32_or("arg3", 0),
                            block.i32_or("arg4", 0),
                        ],
                        id: block.i32_or("id", -1),
                        blocking: block.bool_or("blocking"),
                        blockmonsters: block.bool_or("blockmonsters"),
                        twosided: block.bool_or("twosided"),
                        dontpegtop: block.bool_or("dontpegtop"),
                        dontpegbottom: block.bool_or("dontpegbottom"),
                        secret: block.bool_or("secret"),
                        blocksound: block.bool_or("blocksound"),
                        dontdraw: block.bool_or("dontdraw"),
                        mapped: block.bool_or("mapped"),
                    }),
                    "sidedef" => map.sidedefs.push(UdmfSideDef {
                        offsetx: block.i32_or("offsetx", 0),
                        offsety: block.i32_or("offsety", 0),
                        texturetop: block.texture("texturetop"),
                        texturebottom: block.texture("texturebottom"),
                        texturemiddle: block.texture("texturemiddle"),
                        sector: block.usize_req("sector")?,
                    }),
                    "sector" => map.sectors.push(UdmfSector {
                        heightfloor: block.i32_or("heightfloor", 0),
                        heightceiling: block.i32_or("heightceiling", 0),
                        texturefloor: block.str_req("texturefloor")?,
                        textureceiling: block.str_req("textureceiling")?,
                        lightlevel: block.i32_or("lightlevel", DEFAULT_LIGHT),
                        special: block.i32_or("special", 0),
                        id: block.i32_or("id", 0),
                        floor_plane: block.plane("floorplane"),
                        ceiling_plane: block.plane("ceilingplane"),
                    }),
                    "thing" => map.things.push(UdmfThing {
                        x: block.f64_req("x")?,
                        y: block.f64_req("y")?,
                        height: block.f64_or("height", 0.0),
                        angle: block.i32_or("angle", 0),
                        kind: block
                            .fields
                            .get("type")
                            .and_then(Value::as_i32)
                            .ok_or_else(|| block.err("missing required `type`"))?,
                        ambush: block.bool_or("ambush"),
                        skill1: block.bool_or("skill1"),
                        skill2: block.bool_or("skill2"),
                        skill3: block.bool_or("skill3"),
                        skill4: block.bool_or("skill4"),
                        skill5: block.bool_or("skill5"),
                        single: block.bool_or("single"),
                        dm: block.bool_or("dm"),
                        coop: block.bool_or("coop"),
                    }),
                    _ => {} // unknown block kinds are ignored per spec
                }
            }
            _ => return Err(lexer.err("expected `=` or `{` after identifier")),
        }
    }

    if map.namespace.is_empty() {
        return Err(UdmfError {
            line: 1,
            message: "missing namespace declaration".into(),
        });
    }
    Ok(map)
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Ident(String),
    Value(Value),
    Eq,
    Semi,
    LBrace,
    RBrace,
}

struct Lexer<'a> {
    rest: &'a str,
    line: usize,
    peeked: Option<Token>,
}

impl<'a> Lexer<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            rest: text,
            line: 1,
            peeked: None,
        }
    }

    fn err(&self, message: impl Into<String>) -> UdmfError {
        UdmfError {
            line: self.line,
            message: message.into(),
        }
    }

    fn skip_ws_and_comments(&mut self) -> Result<(), UdmfError> {
        loop {
            let trimmed = self.rest.trim_start_matches(|c: char| {
                if c == '\n' {
                    self.line += 1;
                }
                c.is_whitespace()
            });
            if let Some(stripped) = trimmed.strip_prefix("//") {
                self.rest = stripped.split_once('\n').map_or("", |(_, r)| {
                    self.line += 1;
                    r
                });
            } else if let Some(stripped) = trimmed.strip_prefix("/*") {
                let end = stripped
                    .find("*/")
                    .ok_or_else(|| self.err("unterminated comment"))?;
                self.line += stripped[..end].matches('\n').count();
                self.rest = &stripped[end + 2..];
            } else {
                self.rest = trimmed;
                return Ok(());
            }
        }
    }

    fn peek(&mut self) -> Result<Option<&Token>, UdmfError> {
        if self.peeked.is_none() {
            self.peeked = self.lex()?;
        }
        Ok(self.peeked.as_ref())
    }

    fn next_token(&mut self) -> Result<Option<Token>, UdmfError> {
        if let Some(t) = self.peeked.take() {
            return Ok(Some(t));
        }
        self.lex()
    }

    fn lex(&mut self) -> Result<Option<Token>, UdmfError> {
        self.skip_ws_and_comments()?;
        let mut chars = self.rest.chars();
        let Some(c) = chars.next() else {
            return Ok(None);
        };
        let tok = match c {
            '=' => {
                self.rest = chars.as_str();
                Token::Eq
            }
            ';' => {
                self.rest = chars.as_str();
                Token::Semi
            }
            '{' => {
                self.rest = chars.as_str();
                Token::LBrace
            }
            '}' => {
                self.rest = chars.as_str();
                Token::RBrace
            }
            '"' => {
                let body = chars.as_str();
                let mut value = String::new();
                let mut iter = body.char_indices();
                loop {
                    match iter.next() {
                        None => return Err(self.err("unterminated string")),
                        Some((_, '"')) => {
                            self.rest = iter.as_str();
                            break;
                        }
                        Some((_, '\\')) => match iter.next() {
                            Some((_, esc)) => value.push(esc),
                            None => return Err(self.err("unterminated string escape")),
                        },
                        Some((_, ch)) => value.push(ch),
                    }
                }
                Token::Value(Value::Str(value))
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let end = self
                    .rest
                    .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .unwrap_or(self.rest.len());
                let word = self.rest[..end].to_ascii_lowercase();
                self.rest = &self.rest[end..];
                match word.as_str() {
                    "true" => Token::Value(Value::Bool(true)),
                    "false" => Token::Value(Value::Bool(false)),
                    _ => Token::Ident(word),
                }
            }
            c if c.is_ascii_digit() || c == '+' || c == '-' => {
                let end = self
                    .rest
                    .char_indices()
                    .skip(1)
                    .find(|&(_, c)| {
                        !c.is_ascii_digit()
                            && !matches!(c, '.' | 'x' | 'e' | 'E' | '+' | '-')
                            && !c.is_ascii_hexdigit()
                    })
                    .map_or(self.rest.len(), |(i, _)| i);
                let word = &self.rest[..end];
                self.rest = &self.rest[end..];
                let value = if word.contains('.') || word.contains('e') || word.contains('E') {
                    Value::Float(
                        word.parse()
                            .map_err(|_| self.err(format!("bad float `{word}`")))?,
                    )
                } else if let Some(hex) = word.strip_prefix("0x") {
                    Value::Int(
                        i64::from_str_radix(hex, 16)
                            .map_err(|_| self.err(format!("bad hex `{word}`")))?,
                    )
                } else {
                    Value::Int(
                        word.parse()
                            .map_err(|_| self.err(format!("bad integer `{word}`")))?,
                    )
                };
                Token::Value(value)
            }
            other => return Err(self.err(format!("unexpected character `{other}`"))),
        };
        Ok(Some(tok))
    }

    fn expect(&mut self, want: Token) -> Result<(), UdmfError> {
        match self.next_token()? {
            Some(t) if t == want => Ok(()),
            other => Err(self.err(format!("expected {want:?}, got {other:?}"))),
        }
    }

    fn value(&mut self) -> Result<Value, UdmfError> {
        match self.next_token()? {
            Some(Token::Value(v)) => Ok(v),
            other => Err(self.err(format!("expected a value, got {other:?}"))),
        }
    }

    /// Consume `{ ident = value ; ... }` after the opening brace token.
    fn block(&mut self, kind: &str) -> Result<Block, UdmfError> {
        self.expect(Token::LBrace)?;
        let mut block = Block {
            line: self.line,
            fields: HashMap::new(),
        };
        loop {
            match self.next_token()? {
                Some(Token::RBrace) => return Ok(block),
                Some(Token::Semi) => {} // nil assignment_expr
                Some(Token::Ident(field)) => {
                    self.expect(Token::Eq)?;
                    let value = self.value()?;
                    self.expect(Token::Semi)?;
                    block.fields.insert(field, value);
                }
                other => {
                    return Err(self.err(format!(
                        "in `{kind}` block: expected field or `}}`, got {other:?}"
                    )));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLAT_ROOM: &str = include_str!("../../data/test_files/udmf/flat_room.textmap");
    const SLOPED_FLOOR: &str = include_str!("../../data/test_files/udmf/sloped_floor.textmap");
    const SLOPED_CEILING: &str = include_str!("../../data/test_files/udmf/sloped_ceiling.textmap");

    #[test]
    fn flat_room_parses() {
        let map = parse_textmap(FLAT_ROOM).expect("flat room");
        assert_eq!(map.namespace, "zdoom");
        assert_eq!(map.vertices.len(), 4);
        assert_eq!(map.linedefs.len(), 4);
        assert_eq!(map.sidedefs.len(), 4);
        assert_eq!(map.sectors.len(), 1);
        assert_eq!(map.things.len(), 1);

        let s = &map.sectors[0];
        assert_eq!(s.heightceiling, 128);
        assert_eq!(s.lightlevel, DEFAULT_LIGHT);
        assert_eq!(s.floor_plane, None);
        assert_eq!(s.texturefloor, "FLAT5");

        let ld = &map.linedefs[0];
        assert!(ld.blocking);
        assert_eq!(ld.sideback, None);
        assert_eq!(
            map.sidedefs[ld.sidefront].texturemiddle.as_deref(),
            Some("STONE2")
        );

        let t = &map.things[0];
        assert_eq!(t.kind, 1);
        assert!(t.single);
    }

    #[test]
    fn sloped_floor_plane_fields() {
        let map = parse_textmap(SLOPED_FLOOR).expect("sloped floor");
        let s = &map.sectors[0];
        let plane = s.floor_plane.expect("floor plane present");
        assert_eq!(plane, [-0.25, 0.0, 1.0, 0.0]);
        assert_eq!(s.ceiling_plane, None);
    }

    #[test]
    fn sloped_ceiling_plane_fields() {
        let map = parse_textmap(SLOPED_CEILING).expect("sloped ceiling");
        let s = &map.sectors[0];
        assert_eq!(s.floor_plane, None);
        let plane = s.ceiling_plane.expect("ceiling plane present");
        assert_eq!(plane, [0.25, 0.0, -1.0, 128.0]);
    }

    #[test]
    fn partial_plane_fields_are_ignored() {
        let text = r#"
            namespace = "zdoom";
            sector { texturefloor = "F"; textureceiling = "C"; floorplane_a = 1.0; }
        "#;
        let map = parse_textmap(text).expect("partial plane");
        assert_eq!(map.sectors[0].floor_plane, None);
    }

    #[test]
    fn malformed_input_errors() {
        assert!(
            parse_textmap("vertex { x = 0.0; y = 0.0; }").is_err(),
            "no namespace"
        );
        assert!(
            parse_textmap("namespace = \"zdoom\"; vertex { x = 0.0; }").is_err(),
            "vertex missing y"
        );
        assert!(
            parse_textmap("namespace = \"zdoom\"; sector { texturefloor = \"F\"; }").is_err(),
            "sector missing textureceiling"
        );
        assert!(
            parse_textmap("namespace = \"zdoom\"; linedef { v1 = 0; v2 = 1; }").is_err(),
            "linedef missing sidefront"
        );
        assert!(
            parse_textmap("namespace = \"zdoom\"; }").is_err(),
            "stray brace"
        );
    }

    #[test]
    fn string_escapes_and_nil_assignments() {
        let text = r#"
            namespace = "zdoom";
            ;
            sector { ; texturefloor = "a\"b"; ; textureceiling = "C"; }
        "#;
        let map = parse_textmap(text).expect("escapes and nils");
        assert_eq!(map.sectors[0].texturefloor, "a\"b");
    }

    #[test]
    fn comments_and_case_insensitivity() {
        let text = r#"
            // header comment
            NAMESPACE = "ZDoom";
            /* block
               comment */
            Vertex { X = 1.5; Y = -2.5; }
        "#;
        let map = parse_textmap(text).expect("comments");
        assert_eq!(map.namespace, "zdoom");
        assert_eq!(
            map.vertices[0],
            UdmfVertex {
                x: 1.5,
                y: -2.5
            }
        );
    }
}
