use std::collections::HashMap;

use super::*;

#[derive(Debug)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "UMAPINFO line {}: {}", self.line, self.message)
    }
}
impl std::error::Error for ParseError {}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Ident(String),
    Str(String),
    Num(i32),
    Eq,
    Comma,
    BraceOpen,
    BraceClose,
}

struct Tokenizer<'a> {
    src: &'a str,
    pos: usize,
    line: usize,
}

impl<'a> Tokenizer<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            pos: 0,
            line: 1,
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.peek_char()?;
        self.pos += c.len_utf8();
        if c == '\n' {
            self.line += 1;
        }
        Some(c)
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            match self.peek_char() {
                Some(c) if c.is_ascii_whitespace() => {
                    self.advance();
                }
                Some('/') => {
                    if self.src[self.pos..].starts_with("//") {
                        while let Some(c) = self.advance() {
                            if c == '\n' {
                                break;
                            }
                        }
                    } else if self.src[self.pos..].starts_with("/*") {
                        self.advance();
                        self.advance();
                        while !self.src[self.pos..].starts_with("*/") {
                            if self.advance().is_none() {
                                break;
                            }
                        }
                        self.advance();
                        self.advance();
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }
    }

    fn next_token(&mut self) -> Option<(Token, usize)> {
        self.skip_whitespace_and_comments();
        let line = self.line;
        let c = self.peek_char()?;

        match c {
            '=' => {
                self.advance();
                Some((Token::Eq, line))
            }
            ',' => {
                self.advance();
                Some((Token::Comma, line))
            }
            '{' => {
                self.advance();
                Some((Token::BraceOpen, line))
            }
            '}' => {
                self.advance();
                Some((Token::BraceClose, line))
            }
            '"' => {
                self.advance();
                let start = self.pos;
                while let Some(c) = self.peek_char() {
                    if c == '"' {
                        break;
                    }
                    self.advance();
                }
                let s = self.src[start..self.pos].to_string();
                self.advance(); // consume closing quote
                Some((Token::Str(s), line))
            }
            c if c.is_ascii_digit() || c == '-' => {
                let start = self.pos;
                self.advance();
                while let Some(c) = self.peek_char() {
                    if c.is_ascii_digit() {
                        self.advance();
                    } else {
                        break;
                    }
                }
                let s = &self.src[start..self.pos];
                let n = s.parse().unwrap_or(0);
                Some((Token::Num(n), line))
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let start = self.pos;
                while let Some(c) = self.peek_char() {
                    if c.is_ascii_alphanumeric() || c == '_' {
                        self.advance();
                    } else {
                        break;
                    }
                }
                let s = self.src[start..self.pos].to_string();
                Some((Token::Ident(s), line))
            }
            _ => {
                self.advance();
                self.next_token()
            }
        }
    }
}

struct Parser {
    tokens: Vec<(Token, usize)>,
    pos: usize,
}

impl Parser {
    fn new(src: &str) -> Self {
        let mut tokenizer = Tokenizer::new(src);
        let mut tokens = Vec::new();
        while let Some(tok) = tokenizer.next_token() {
            tokens.push(tok);
        }
        Self {
            tokens,
            pos: 0,
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|(t, _)| t)
    }

    fn line(&self) -> usize {
        self.tokens.get(self.pos).map_or(0, |(_, l)| *l)
    }

    fn advance(&mut self) -> Option<&Token> {
        let tok = self.tokens.get(self.pos).map(|(t, _)| t);
        self.pos += 1;
        tok
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        match self.advance() {
            Some(Token::Ident(s)) => Ok(s.clone()),
            _ => Err(ParseError {
                line: self.line(),
                message: "expected identifier".into(),
            }),
        }
    }

    fn expect_string(&mut self) -> Result<String, ParseError> {
        match self.advance() {
            Some(Token::Str(s)) => Ok(s.clone()),
            Some(Token::Ident(s)) => Ok(s.clone()),
            _ => Err(ParseError {
                line: self.line(),
                message: "expected string".into(),
            }),
        }
    }

    fn expect(&mut self, expected: &Token) -> Result<(), ParseError> {
        let line = self.line();
        match self.advance() {
            Some(t) if t == expected => Ok(()),
            Some(t) => Err(ParseError {
                line,
                message: format!("expected {:?}, got {:?}", expected, t),
            }),
            None => Err(ParseError {
                line,
                message: format!("expected {:?}, got EOF", expected),
            }),
        }
    }

    fn parse_multi_string(&mut self) -> Result<String, ParseError> {
        let first = self.expect_string()?;
        let mut lines = vec![first];
        while self.peek() == Some(&Token::Comma) {
            self.advance();
            lines.push(self.expect_string()?);
        }
        Ok(lines.join("\n"))
    }

    fn parse_string_or_clear(&mut self) -> Result<TextOrClear, ParseError> {
        if matches!(self.peek(), Some(Token::Ident(s)) if s.eq_ignore_ascii_case("clear")) {
            self.advance();
            return Ok(TextOrClear::Clear);
        }
        Ok(TextOrClear::Text(self.parse_multi_string()?))
    }

    fn parse_label(&mut self) -> Result<LabelKind, ParseError> {
        if matches!(self.peek(), Some(Token::Ident(s)) if s.eq_ignore_ascii_case("clear")) {
            self.advance();
            return Ok(LabelKind::Clear);
        }
        Ok(LabelKind::Text(self.expect_string()?))
    }

    fn parse_bool(&mut self) -> Result<bool, ParseError> {
        let line = self.line();
        match self.advance() {
            Some(Token::Ident(s)) if s.eq_ignore_ascii_case("true") => Ok(true),
            Some(Token::Ident(s)) if s.eq_ignore_ascii_case("false") => Ok(false),
            _ => Err(ParseError {
                line,
                message: "expected true or false".into(),
            }),
        }
    }

    fn parse_num(&mut self) -> Result<i32, ParseError> {
        let line = self.line();
        match self.advance() {
            Some(Token::Num(n)) => Ok(*n),
            _ => Err(ParseError {
                line,
                message: "expected number".into(),
            }),
        }
    }

    fn parse_episode_def(&mut self) -> Result<Option<EpisodeDef>, ParseError> {
        if matches!(self.peek(), Some(Token::Ident(s)) if s.eq_ignore_ascii_case("clear")) {
            self.advance();
            return Ok(None);
        }
        let patch = self.expect_string()?;
        self.expect(&Token::Comma)?;
        let name = self.expect_string()?;
        self.expect(&Token::Comma)?;
        let key = self.expect_string()?;
        Ok(Some(EpisodeDef {
            patch,
            name,
            key,
        }))
    }

    fn parse_boss_action(&mut self, entry: &mut MapEntry) -> Result<(), ParseError> {
        if matches!(self.peek(), Some(Token::Ident(s)) if s.eq_ignore_ascii_case("clear")) {
            self.advance();
            entry.boss_actions = Some(BossActions::Clear);
            return Ok(());
        }
        let thing_type = self.expect_ident()?;
        self.expect(&Token::Comma)?;
        let line_special = self.parse_num()?;
        self.expect(&Token::Comma)?;
        let tag = self.parse_num()?;
        let action = BossAction {
            thing_type,
            line_special,
            tag,
        };
        match &mut entry.boss_actions {
            Some(BossActions::Actions(actions)) => actions.push(action),
            _ => entry.boss_actions = Some(BossActions::Actions(vec![action])),
        }
        Ok(())
    }

    fn parse_entry(&mut self) -> Result<MapEntry, ParseError> {
        let map_name = self.expect_ident()?.to_ascii_uppercase();
        let (episode, map) = parse_map_name(&map_name);

        // ZMAPINFO: optional quoted level name before brace
        let mut level_name = None;
        if matches!(self.peek(), Some(Token::Str(_))) {
            level_name = Some(self.expect_string()?);
        }

        self.expect(&Token::BraceOpen)?;

        let mut entry = MapEntry {
            map_name,
            episode,
            map,
            level_name,
            ..Default::default()
        };

        while self.peek() != Some(&Token::BraceClose) && self.peek().is_some() {
            let key_line = self.line();
            let key = self.expect_ident()?.to_ascii_lowercase();

            // ZMAPINFO flag keys (no = or value)
            let is_flag = matches!(
                key.as_str(),
                "map07special"
                    | "allowmonstertelefrags"
                    | "resetinventory"
                    | "resethealth"
                    | "nojump"
                    | "nocrouch"
                    | "nofreelook"
            );

            if is_flag {
                match key.as_str() {
                    "map07special" => {
                        entry.boss_actions = Some(BossActions::Actions(vec![
                            BossAction {
                                thing_type: "Fatso".into(),
                                line_special: 38,
                                tag: 666,
                            },
                            BossAction {
                                thing_type: "Arachnotron".into(),
                                line_special: 30,
                                tag: 667,
                            },
                        ]));
                    }
                    _ => {} // skip other flags
                }
                continue;
            }

            // Consume optional = (ZMAPINFO uses =, some keys in old format don't)
            if self.peek() == Some(&Token::Eq) {
                self.advance();
            }

            match key.as_str() {
                "levelname" => entry.level_name = Some(self.expect_string()?),
                "label" => entry.label = Some(self.parse_label()?),
                "author" => entry.author = Some(self.expect_string()?),
                "levelpic" | "titlepatch" => entry.level_pic = Some(self.expect_string()?),
                "next" => entry.next = Some(self.expect_string()?),
                "nextsecret" | "secretnext" => entry.next_secret = Some(self.expect_string()?),
                "skytexture" => entry.sky_texture = Some(self.expect_string()?),
                "sky1" => {
                    entry.sky_texture = Some(self.expect_string()?);
                    // Skip optional scroll rate
                    if matches!(self.peek(), Some(Token::Num(_))) {
                        self.advance();
                    }
                }
                "music" => entry.music = Some(self.expect_string()?),
                "exitpic" => entry.exit_pic = Some(self.expect_string()?),
                "enterpic" => entry.enter_pic = Some(self.expect_string()?),
                "partime" | "par" => entry.par_time = Some(self.parse_num()?),
                "endgame" => entry.end_game = Some(self.parse_bool()?),
                "endpic" => entry.end_pic = Some(self.expect_string()?),
                "endbunny" => entry.end_bunny = self.parse_bool()?,
                "endcast" => entry.end_cast = self.parse_bool()?,
                "nointermission" => entry.no_intermission = self.parse_bool()?,
                "intertext" => entry.inter_text = Some(self.parse_string_or_clear()?),
                "intertextsecret" => entry.inter_text_secret = Some(self.parse_string_or_clear()?),
                "interbackdrop" => entry.inter_backdrop = Some(self.expect_string()?),
                "intermusic" => entry.inter_music = Some(self.expect_string()?),
                "episode" => entry.episode_def = self.parse_episode_def()?,
                "bossaction" => self.parse_boss_action(&mut entry)?,
                "cluster" => entry.cluster_id = Some(self.parse_num()?),
                "levelnum" | "translator" => {
                    self.skip_value();
                }
                unknown => {
                    log::warn!("MAPINFO line {}: unknown key '{}'", key_line, unknown);
                    self.skip_value();
                }
            }
        }

        self.expect(&Token::BraceClose)?;
        Ok(entry)
    }

    fn skip_value(&mut self) {
        while let Some(t) = self.peek() {
            match t {
                Token::BraceClose => break,
                Token::Ident(_) => {
                    if self.tokens.get(self.pos + 1).map(|(t, _)| t) == Some(&Token::Eq) {
                        break;
                    }
                    // Also break if next ident looks like a flag key
                    let next_is_flag = matches!(t, Token::Ident(s) if matches!(s.to_ascii_lowercase().as_str(),
                        "map07special" | "allowmonstertelefrags" | "resetinventory" |
                        "resethealth" | "nojump" | "nocrouch" | "nofreelook"
                    ));
                    if next_is_flag {
                        break;
                    }
                    self.advance();
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    fn skip_braced_block(&mut self) {
        if self.peek() == Some(&Token::BraceOpen) {
            self.advance();
            let mut depth = 1u32;
            while depth > 0 {
                match self.advance() {
                    Some(Token::BraceOpen) => depth += 1,
                    Some(Token::BraceClose) => depth -= 1,
                    None => break,
                    _ => {}
                }
            }
        }
    }

    fn parse_cluster(&mut self) -> Result<(i32, ClusterDef), ParseError> {
        let id = self.parse_num()?;
        self.expect(&Token::BraceOpen)?;

        let mut cluster = ClusterDef::default();

        while self.peek() != Some(&Token::BraceClose) && self.peek().is_some() {
            let key = self.expect_ident()?.to_ascii_lowercase();
            if self.peek() == Some(&Token::Eq) {
                self.advance();
            }
            match key.as_str() {
                "flat" => cluster.flat = Some(self.expect_string()?),
                "music" => cluster.music = Some(self.expect_string()?),
                "exittext" => cluster.exit_text = Some(self.parse_multi_string()?),
                "entertext" => cluster.enter_text = Some(self.parse_multi_string()?),
                _ => {
                    self.skip_value();
                }
            }
        }
        self.expect(&Token::BraceClose)?;
        Ok((id, cluster))
    }

    fn parse_zmapinfo_episode(&mut self) -> Result<(String, EpisodeDef), ParseError> {
        let map_name = self.expect_ident()?.to_ascii_uppercase();
        self.expect(&Token::BraceOpen)?;

        let mut name = String::new();
        let mut patch = String::new();
        let mut key = String::new();

        while self.peek() != Some(&Token::BraceClose) && self.peek().is_some() {
            let k = self.expect_ident()?.to_ascii_lowercase();
            if self.peek() == Some(&Token::Eq) {
                self.advance();
            }
            match k.as_str() {
                "name" => name = self.expect_string()?,
                "picname" => patch = self.expect_string()?,
                "key" => key = self.expect_string()?,
                _ => {
                    self.skip_value();
                }
            }
        }
        self.expect(&Token::BraceClose)?;
        Ok((
            map_name,
            EpisodeDef {
                patch,
                name,
                key,
            },
        ))
    }
}

#[derive(Debug, Default)]
struct ClusterDef {
    flat: Option<String>,
    music: Option<String>,
    exit_text: Option<String>,
    enter_text: Option<String>,
}

pub fn parse(input: &str) -> Result<UMapInfo, ParseError> {
    let mut parser = Parser::new(input);
    let mut entries = Vec::new();
    let mut index = HashMap::new();
    let mut clusters: HashMap<i32, ClusterDef> = HashMap::new();
    let mut episode_defs: Vec<(String, EpisodeDef)> = Vec::new();
    let mut clear_episodes = false;

    while let Some(tok) = parser.peek() {
        match tok {
            Token::Ident(s) if s.eq_ignore_ascii_case("map") => {
                parser.advance();
                let entry = parser.parse_entry()?;
                let key = entry.map_name.clone();
                let idx = entries.len();
                entries.push(entry);
                index.insert(key, idx);
            }
            Token::Ident(s) if s.eq_ignore_ascii_case("cluster") => {
                parser.advance();
                let (id, cluster) = parser.parse_cluster()?;
                clusters.insert(id, cluster);
            }
            Token::Ident(s) if s.eq_ignore_ascii_case("episode") => {
                parser.advance();
                let (map_name, ep_def) = parser.parse_zmapinfo_episode()?;
                episode_defs.push((map_name, ep_def));
            }
            Token::Ident(s) if s.eq_ignore_ascii_case("clearepisodes") => {
                parser.advance();
                clear_episodes = true;
            }
            Token::Ident(s)
                if s.eq_ignore_ascii_case("defaultmap") || s.eq_ignore_ascii_case("gameinfo") =>
            {
                parser.advance();
                parser.skip_braced_block();
            }
            Token::Ident(s) if s.eq_ignore_ascii_case("intermission") => {
                parser.advance();
                // Skip the intermission name ident
                if matches!(parser.peek(), Some(Token::Ident(_))) {
                    parser.advance();
                }
                parser.skip_braced_block();
            }
            _ => {
                parser.advance();
            }
        }
    }

    // Resolve cluster references: copy cluster data onto map entries
    for entry in &mut entries {
        if let Some(cid) = entry.cluster_id {
            if let Some(cluster) = clusters.get(&cid) {
                if entry.inter_backdrop.is_none() {
                    entry.inter_backdrop = cluster.flat.clone();
                }
                if entry.inter_music.is_none() {
                    entry.inter_music = cluster.music.clone();
                }
                if entry.inter_text.is_none() {
                    if let Some(text) = &cluster.exit_text {
                        entry.inter_text = Some(TextOrClear::Text(text.clone()));
                    }
                }
            }
        }
    }

    // Resolve episode definitions: attach to the first map entry they reference
    for (map_name, ep_def) in episode_defs {
        let key = map_name.to_ascii_uppercase();
        if let Some(&idx) = index.get(&key) {
            entries[idx].episode_def = Some(ep_def);
        }
    }

    Ok(UMapInfo {
        entries,
        index,
        clear_episodes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIGIL2_UMAPINFO: &str = r#"
// UMAPINFO for SIGIL II

map E6M1
{
	LevelName = "Cursed Darkness"
	LevelPic = "WILV50"
	Next = "E6M2"
	Music = "D_E6M1"
	SkyTexture = "SKY6"
	ExitPic = "SIGILIN2"
	Episode = "M_EPI6", "SIGIL II", "S"
	partime = 480
}

map E6M2
{
	LevelName = "Violent Hatred"
	LevelPic = "WILV51"
	Next = "E6M3"
	Music = "D_E6M2"
	SkyTexture = "SKY6"
	ExitPic = "SIGILIN2"
	partime = 300
}

map E6M3
{
	LevelName = "Twilight Desolation"
	LevelPic = "WILV52"
	Next = "E6M4"
	NextSecret = "E6M9"
	Music = "D_E6M3"
	SkyTexture = "SKY6"
	ExitPic = "SIGILIN2"
	partime = 240
}

map E6M4
{
	LevelName = "Fragments of Sanity"
	LevelPic = "WILV53"
	Next = "E6M5"
	Music = "D_E6M4"
	SkyTexture = "SKY6"
	ExitPic = "SIGILIN2"
	partime = 420
}

map E6M5
{
	LevelName = "Wrathful Reckoning"
	LevelPic = "WILV54"
	Next = "E6M6"
	Music = "D_E6M5"
	SkyTexture = "SKY6"
	ExitPic = "SIGILIN2"
	partime = 510
}

map E6M6
{
	LevelName = "Vengeance Unleashed"
	LevelPic = "WILV55"
	Next = "E6M7"
	Music = "D_E6M6"
	SkyTexture = "SKY6"
	ExitPic = "SIGILIN2"
	partime = 840
}

map E6M7
{
	LevelName = "Descent Into Terror"
	LevelPic = "WILV56"
	Next = "E6M8"
	Music = "D_E6M7"
	SkyTexture = "SKY6"
	ExitPic = "SIGILIN2"
	partime = 960
}

map E6M8
{
	LevelName = "Abyss of Despair"
	LevelPic = "WILV57"
	EndPic = "CREDIT"
	InterText = "Satan erred in casting you to Hell's",
				"darker depths. His plan failed. He has",
				"tried for so long to destroy you, and he",
				"has lost every single time. His only",
				"option is to flood Earth with demons",
				"and hope you go down fighting.",
				"",
				"Prepare for HELLION!"
	Music = "D_E6M8"
	SkyTexture = "SKY6"
	ExitPic = "SIGILIN2"
	BossAction = clear
	partime = 390
}

map E6M9
{
	LevelName = "Shattered Homecoming"
	LevelPic = "WILV58"
	Next = "E6M4"
	Music = "D_E6M9"
	SkyTexture = "SKY6"
	ExitPic = "SIGILIN2"
	partime = 450
}
"#;

    #[test]
    fn test_sigil2_umapinfo() {
        let info = parse(SIGIL2_UMAPINFO).expect("parse failed");
        assert_eq!(info.entries.len(), 9);

        // E6M1
        let e6m1 = info.get("E6M1").expect("E6M1 missing");
        assert_eq!(e6m1.level_name.as_deref(), Some("Cursed Darkness"));
        assert_eq!(e6m1.level_pic.as_deref(), Some("WILV50"));
        assert_eq!(e6m1.next.as_deref(), Some("E6M2"));
        assert_eq!(e6m1.music.as_deref(), Some("D_E6M1"));
        assert_eq!(e6m1.sky_texture.as_deref(), Some("SKY6"));
        assert_eq!(e6m1.exit_pic.as_deref(), Some("SIGILIN2"));
        assert_eq!(e6m1.par_time, Some(480));
        assert_eq!(e6m1.episode, 6);
        assert_eq!(e6m1.map, 1);
        let ep = e6m1.episode_def.as_ref().expect("episode_def missing");
        assert_eq!(ep.patch, "M_EPI6");
        assert_eq!(ep.name, "SIGIL II");
        assert_eq!(ep.key, "S");

        // E6M3 — has secret exit
        let e6m3 = info.get("E6M3").expect("E6M3 missing");
        assert_eq!(e6m3.next.as_deref(), Some("E6M4"));
        assert_eq!(e6m3.next_secret.as_deref(), Some("E6M9"));

        // E6M8 — end pic, intertext, boss action clear
        let e6m8 = info.get("E6M8").expect("E6M8 missing");
        assert_eq!(e6m8.end_pic.as_deref(), Some("CREDIT"));
        match &e6m8.inter_text {
            Some(TextOrClear::Text(t)) => {
                let lines: Vec<&str> = t.lines().collect();
                assert_eq!(lines.len(), 8);
                assert_eq!(lines[0], "Satan erred in casting you to Hell's");
                assert_eq!(lines[6], "");
                assert_eq!(lines[7], "Prepare for HELLION!");
            }
            other => panic!("expected Text, got {:?}", other),
        }
        assert!(matches!(&e6m8.boss_actions, Some(BossActions::Clear)));

        // E6M9 — loops back to E6M4
        let e6m9 = info.get("E6M9").expect("E6M9 missing");
        assert_eq!(e6m9.next.as_deref(), Some("E6M4"));
        assert_eq!(e6m9.par_time, Some(450));

        // Lookup by episode/map
        let by_ep = info.get_by_ep_map(6, 5).expect("ep 6 map 5 missing");
        assert_eq!(by_ep.level_name.as_deref(), Some("Wrathful Reckoning"));

        // Episodes
        let episodes = info.episodes();
        assert_eq!(episodes.len(), 1);
        assert_eq!(episodes[0].name, "SIGIL II");
    }

    #[test]
    fn test_empty_input() {
        let info = parse("").expect("parse failed");
        assert_eq!(info.entries.len(), 0);
    }

    #[test]
    fn test_empty_with_comments() {
        let info = parse("// just a comment\n// another one\n").expect("parse failed");
        assert_eq!(info.entries.len(), 0);
    }

    #[test]
    fn test_unknown_keys() {
        let input = r#"
map E1M1
{
    levelname = "Test"
    unknownkey = "ignored"
    partime = 60
}
"#;
        let info = parse(input).expect("parse failed");
        let entry = info.get("E1M1").expect("E1M1 missing");
        assert_eq!(entry.level_name.as_deref(), Some("Test"));
        assert_eq!(entry.par_time, Some(60));
    }

    #[test]
    fn test_multiline_intertext() {
        let input = r#"
map MAP01
{
    intertext = "Line one",
                "Line two",
                "Line three"
}
"#;
        let info = parse(input).expect("parse failed");
        let entry = info.get("MAP01").expect("MAP01 missing");
        match &entry.inter_text {
            Some(TextOrClear::Text(t)) => {
                assert_eq!(t, "Line one\nLine two\nLine three");
            }
            other => panic!("expected Text, got {:?}", other),
        }
        assert_eq!(entry.episode, 0);
        assert_eq!(entry.map, 1);
    }

    #[test]
    fn test_intertext_clear() {
        let input = r#"
map MAP06
{
    intertext = clear
}
"#;
        let info = parse(input).expect("parse failed");
        let entry = info.get("MAP06").expect("MAP06 missing");
        assert!(matches!(&entry.inter_text, Some(TextOrClear::Clear)));
    }

    #[test]
    fn test_boss_actions() {
        let input = r#"
map E1M8
{
    bossaction = BaronOfHell, 23, 666
    bossaction = Cyberdemon, 30, 1
}
"#;
        let info = parse(input).expect("parse failed");
        let entry = info.get("E1M8").expect("E1M8 missing");
        match &entry.boss_actions {
            Some(BossActions::Actions(actions)) => {
                assert_eq!(actions.len(), 2);
                assert_eq!(actions[0].thing_type, "BaronOfHell");
                assert_eq!(actions[0].line_special, 23);
                assert_eq!(actions[0].tag, 666);
                assert_eq!(actions[1].thing_type, "Cyberdemon");
            }
            other => panic!("expected Actions, got {:?}", other),
        }
    }

    #[test]
    fn test_case_insensitive_keys() {
        let input = r#"
MAP e1m1
{
    LEVELNAME = "Test"
    ParTime = 120
    SkyTexture = "SKY2"
}
"#;
        let info = parse(input).expect("parse failed");
        let entry = info.get("E1M1").expect("E1M1 missing");
        assert_eq!(entry.level_name.as_deref(), Some("Test"));
        assert_eq!(entry.par_time, Some(120));
        assert_eq!(entry.sky_texture.as_deref(), Some("SKY2"));
    }

    #[test]
    fn test_sigil2_umapinfo_file() {
        let data = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../data/test_files/umapinfo/sigil2_umapinfo.txt"
        ))
        .expect("sigil2_umapinfo.txt");
        let info = parse(&data).expect("parse failed");
        assert_eq!(info.entries.len(), 9);
        let e6m1 = info.get("E6M1").expect("E6M1 missing");
        assert_eq!(e6m1.level_name.as_deref(), Some("Cursed Darkness"));
        assert_eq!(e6m1.music.as_deref(), Some("D_E6M1"));
        assert_eq!(e6m1.sky_texture.as_deref(), Some("SKY6"));
    }

    #[test]
    fn test_sigil_umapinfo_file() {
        let data = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../data/test_files/umapinfo/sigil_umapinfo.txt"
        ))
        .expect("sigil_umapinfo.txt");
        let info = parse(&data).expect("parse failed");
        assert!(!info.entries.is_empty());
        let e5m1 = info.get("E5M1").expect("E5M1 missing");
        assert!(e5m1.level_name.is_some());
    }

    #[test]
    fn test_sos_boom_umapinfo_file() {
        let data = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../data/test_files/umapinfo/sos_boom_umapinfo.txt"
        ))
        .expect("sos_boom_umapinfo.txt");
        let info = parse(&data).expect("parse failed");
        assert!(!info.entries.is_empty());
        assert!(info.get("MAP01").is_some());
    }

    #[test]
    fn test_eviternity_zmapinfo() {
        let data = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../data/test_files/mapinfo/eviternity_zmapinfo.txt"
        ))
        .expect("eviternity_zmapinfo.txt");
        let info = parse(&data).expect("parse failed");

        // 32 map entries
        assert_eq!(info.entries.len(), 32);

        // MAP01 basics
        let m01 = info.get("MAP01").expect("MAP01 missing");
        assert_eq!(m01.level_name.as_deref(), Some("inauguration"));
        assert_eq!(m01.level_pic.as_deref(), Some("CWILV00"));
        assert_eq!(m01.next.as_deref(), Some("MAP02"));
        assert_eq!(m01.next_secret.as_deref(), Some("MAP02"));
        assert_eq!(m01.sky_texture.as_deref(), Some("OSKY28"));
        assert_eq!(m01.music.as_deref(), Some("D_RUNNIN"));
        assert_eq!(m01.par_time, Some(35));
        assert_eq!(m01.cluster_id, Some(5));

        // MAP07 has map07special
        let m07 = info.get("MAP07").expect("MAP07 missing");
        assert!(m07.boss_actions.is_some());

        // MAP15 has secret exit to MAP31
        let m15 = info.get("MAP15").expect("MAP15 missing");
        assert_eq!(m15.next.as_deref(), Some("MAP16"));
        assert_eq!(m15.next_secret.as_deref(), Some("MAP31"));

        // MAP30 has no next (end of game)
        let m30 = info.get("MAP30").expect("MAP30 missing");
        assert!(m30.next.is_none());

        // Cluster resolution: MAP05 is in cluster 5, which has exittext
        let m05 = info.get("MAP05").expect("MAP05 missing");
        assert_eq!(m05.inter_backdrop.as_deref(), Some("OGRATB02"));
        assert_eq!(m05.inter_music.as_deref(), Some("D_READ_M"));
        assert!(m05.inter_text.is_some());

        // clearepisodes
        assert!(info.clear_episodes);

        // Episode definitions attached to maps
        let ep1 = info.get("MAP01").unwrap();
        assert!(ep1.episode_def.is_some());
        let ep_def = ep1.episode_def.as_ref().unwrap();
        assert_eq!(ep_def.name, "Chapter I: Archaic");
        assert_eq!(ep_def.patch, "M_EPI1");

        // 6 episodes total
        assert_eq!(info.episodes().len(), 6);
    }

    #[test]
    fn test_block_comments() {
        let input = r#"
/* this is a block comment */
map MAP01 /* inline comment */ {
    levelname = "Test"
    /* multi
       line
       comment */
    partime = 60
}
"#;
        let info = parse(input).expect("parse failed");
        let entry = info.get("MAP01").expect("MAP01 missing");
        assert_eq!(entry.level_name.as_deref(), Some("Test"));
        assert_eq!(entry.par_time, Some(60));
    }
}
