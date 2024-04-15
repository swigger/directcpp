use std::os::raw::c_char;
use std::ffi::{CStr};
use std::collections::{HashMap};
use std::ops::{Range, RangeInclusive};
use proc_macro2::{TokenStream, TokenTree};
use regex::Regex;

extern "C" {
    fn fmt_g_f64(s: *mut c_char, len: usize, val:f64) -> i32;
}

pub fn reformat_number(s: &str) -> String {
    let ns = if let Ok(float_num) = s.parse::<f64>() {
        let mut buffer: [c_char; 64] = [0; 64];
        let result = unsafe {
            fmt_g_f64(buffer.as_mut_ptr(), 64, float_num);
            CStr::from_ptr(buffer.as_ptr()).to_string_lossy().to_string()
        };
        result
    } else if let Ok(uint_num) = s.parse::<u64>() {
        format!("{}", uint_num) // Format as u64
    } else {
        String::from("0")
    };
    ns
}

#[derive(Default)]
pub struct CharMapper {
    ss: &'static str,
    rg: Range<char>,
    idx: usize,
    pub mpcs: HashMap<char, String>,
    pub mpsc: HashMap<String, char>,
}

impl CharMapper {
    pub fn new(ss: &'static str, from: u32, to: u32) -> Self {
        Self {
            ss,
            rg: unsafe{ char::from_u32_unchecked(from) ..char::from_u32_unchecked(to) },
            idx: 0,
            mpcs: HashMap::new(),
            mpsc: HashMap::new(),
        }
    }
    pub fn next(&mut self) -> Option<char> {
        if self.idx < self.ss.len() {
            self.idx += 1;
            self.ss.chars().nth(self.idx-1)
        } else {
            let idx2 = self.idx - self.ss.len();
            if idx2 < self.rg.end as usize - self.rg.start as usize {
                self.idx += 1;
                let val = self.rg.start as u32 + idx2 as u32;
                unsafe { Some(char::from_u32_unchecked(val)) }
            } else {
                None
            }
        }
    }
    pub fn add_string(&mut self, s: String) -> char {
        if let Some(c) = self.mpsc.get(&s) {
            return *c;
        }
        let c = self.next().unwrap();
        self.mpcs.insert(c, s.to_string());
        self.mpsc.insert(s, c);
        c
    }
    pub fn any_reg(&self) -> String {
        let commit = |ret:&mut String, r:&RangeInclusive<char>| {
            if (*r.start() as u32) + 2 >= (*r.end() as u32) {
                for k in *r.start() ..= *r.end() {
                    ret.push(k);
                }
            } else {
                ret.push(*r.start());
                ret.push('-');
                ret.push(*r.end());
            }
        };
        let mut ret = String::new();
        let mut cur:Option<RangeInclusive<char>> = None;
        for j in 0..self.idx {
            let ch = if j < self.ss.len() {
                self.ss.chars().nth(j).unwrap()
            } else {
                let idx2 = j - self.ss.len();
                let val = self.rg.start as u32 + idx2 as u32;
                unsafe { char::from_u32_unchecked(val) }
            };
            // put ch into cur
            match cur {
                Some(r) => {
                    if *r.end() as u32 + 1 == ch as u32 {
                        cur = Some(*r.start()..=ch);
                    } else {
                        commit(&mut ret, &r);
                        cur = Some(ch..=ch);
                    }
                },
                None => { cur.replace(ch..=ch); }
            };
        }
        if let Some(r) = cur {
            commit(&mut ret, &r);
        }
        return format!("[{}]", ret);
    }
}

enum CMToken<'a> {
    Ident(&'a str),
    Literal(&'a str),
    Number(&'a str),
    Punct(char),
    Ticked(&'a str),
}

fn is_iden(ch: char, lead: bool) -> bool {
    if ch > '\x7f' {
        return false;
    }
    if ch.is_alphabetic() || ch == '_' {
        return true;
    }
    if !lead && ch >= '0' && ch <= '9' {
        return true;
    }
    false
}

fn tokenize(ts0: &str) -> Option<Vec<CMToken>> {
    let mut ret = Vec::new();
    let mut ts = ts0;
    while ! ts.is_empty() {
        let mut ch = ts.chars().next().unwrap();
        // if ch is space, continue search .
        if ch.is_whitespace() {
            ts = &ts[1..];
            continue;
        }
        // if ch is a letter, search for a word.
        if is_iden(ch, true) {
            let mut i = 1;
            while i < ts.len() && is_iden(ts.chars().nth(i).unwrap(), false) {
                i += 1;
            }
            let word = &ts[..i];
            ret.push(CMToken::Ident(word));
            ts = &ts[i..];
            continue;
        }
        // if ch is number, search for a number.
        if ch.is_numeric() {
            let mut i = 1;
            while i < ts.len() && ts.chars().nth(i).unwrap().is_alphanumeric() {
                i += 1;
            }
            let word = &ts[..i];
            ret.push(CMToken::Number(word));
            ts = &ts[i..];
            continue;
        }
        // if ch is '"' or ', we should read a literal string.
        if ch == '\'' || ch == '"' {
            let mut i = 1;
            let mut found_tail = false;
            while i < ts.len() {
                if ts.chars().nth(i).unwrap() != ch {
                    i += 1;
                } else {
                    found_tail = true;
                    break;
                }
            }
            if !found_tail {
                return None;
            }
            let word = &ts[..i+1];
            ret.push(CMToken::Literal(word));
            ts = &ts[i+1..];
            continue;
        }
        // on tick, we read ticked punctuations.
        if ch == '`' {
            let mut i = 1;
            if i < ts.len() && {ch = ts.chars().nth(i).unwrap(); ch} == '`' {
                ts = &ts[2..];
                ret.push(CMToken::Ticked("`"));
                continue;
            }
            while i < ts.len() {
                ch = ts.chars().nth(i).unwrap();
                if ! ch.is_whitespace() && ch != '`' {
                    i += 1;
                } else {
                    break;
                }
            }
            if i > 1 {
                ret.push(CMToken::Ticked(&ts[1..i]));
            }
            ts = &ts[i..];
            continue;
        }

        // if ch is a punct, present it
        if ch.is_ascii_punctuation() {
            ret.push(CMToken::Punct(ch));
            ts = &ts[1..];
            continue;
        }
        // we meet an unknown char. warning and skip it.
        println!("unknown: {}", ch);
        ts = &ts[1..];
    }
    Some(ret)
}

#[derive(Default)]
pub struct CodeMatchObject {
    kw: CharMapper,
    ident: CharMapper,
    number: CharMapper,
    literal: CharMapper,
    punct: CharMapper,
    group: CharMapper,
    vts: Vec<TokenStream>
}

impl CodeMatchObject {
    pub fn new() -> Self {
        let ascii = "_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        // Ā~ſ (Latin Extended-A)
        let mut mapper_kw = CharMapper::new(ascii, 0x100, 0x17f);
        let all_kws = ["abstract", "as", "async", "await", "become", "box", "break",
            "const", "continue", "crate", "do", "dyn", "else", "enum", "extern", "false",
            "final", "fn", "for", "if", "impl", "in", "let", "loop", "macro", "macro_rules",
            "match", "mod", "move", "mut", "override", "priv", "pub", "ref", "return", "self",
            "Self", "static", "struct", "super", "trait", "true", "try", "type", "typeof",
            "union", "unsafe", "unsized", "use", "virtual", "where", "while", "yield"];
        for kw in all_kws.iter() {
            mapper_kw.add_string(kw.to_string());
        }

        Self { // usable: 2800, 1200,
            kw: mapper_kw,
            ident: CharMapper::new("", 0x3400,0x4DBF),  // CJK Unified Ideographs Extension A
            number: CharMapper::new("", 0x2801, 0x28ff),  // Braille Patterns
            literal: CharMapper::new("", 0xac00, 0xd7af+1),  // Hangul Syllables
            punct: CharMapper::new("",0x2900, 0x297f+1),  // Supplemental Arrows-B
            group: CharMapper::new("",0x1f600, 0x1f64f+1),  // Emoticons
            vts: Vec::new(),
        }
    }

    pub fn add_number(self: &mut Self, s: &str) -> char {
        self.number.add_string(reformat_number(s))
    }

    fn append_to(self: &mut Self, os: &mut String, ts: TokenStream, expand: bool) {
        for tt in ts {
            match tt {
                TokenTree::Ident(x) => {
                    let xid = x.to_string();
                    if let Some(c) = self.kw.mpsc.get(&xid) {
                        os.push(*c);
                    } else if xid.chars().next().unwrap().is_numeric() {
                        os.push(self.add_number(&xid));
                    } else {
                        os.push(self.ident.add_string(xid));
                    }
                },
                TokenTree::Literal(x) => {
                    os.push(self.literal.add_string(x.to_string()));
                },
                TokenTree::Punct(x) => {
                    os.push(self.punct.add_string(x.to_string()));
                },
                TokenTree::Group(x) => {
                    if expand {
                        match x.delimiter() {
                            proc_macro2::Delimiter::Brace => {
                                os.push(self.punct.add_string("{".to_string()));
                                self.append_to(os, x.stream(), expand);
                                os.push(self.punct.add_string("}".to_string()));
                            },
                            proc_macro2::Delimiter::Parenthesis => {
                                os.push(self.punct.add_string("(".to_string()));
                                self.append_to(os, x.stream(), expand);
                                os.push(self.punct.add_string(")".to_string()));
                            },
                            proc_macro2::Delimiter::Bracket => {
                                os.push(self.punct.add_string("[".to_string()));
                                self.append_to(os, x.stream(), expand);
                                os.push(self.punct.add_string("]".to_string()));
                            },
                            _ => {},
                        }
                    } else  {
                        // if not expand, we always add a group id after the group char.
                        let mapch = match x.delimiter() {
                            proc_macro2::Delimiter::Brace => '{',
                            proc_macro2::Delimiter::Parenthesis => '(',
                            proc_macro2::Delimiter::Bracket => '[',
                            _ => '\0',
                        };
                        let mapch = self.group.add_string(mapch.to_string());
                        os.push(mapch);
                        let grp_id = self.vts.len();
                        self.vts.push(x.stream());
                        os.push_str(&format!("{}", grp_id));
                    }
                },
            }
        }
    }

    pub fn build_string(self: &mut Self, ts: TokenStream, expand: bool) -> String {
        let mut ss = String::new();
        self.append_to(&mut ss, ts, expand);
        ss
    }

    pub fn build_regex(self: &mut Self, ts: &str, expand_form: bool) -> Option<String> {
        let mut grp_stack:Vec<char> = Vec::new();
        let mut ret = String::new();
        let present = |stk:&Vec<char>, ss:&mut String, x: char| {
            if expand_form || stk.is_empty() {
                ss.push(x);
            }
        };
        let present_str = |stk:&Vec<char>, ss:&mut String, x: &str| {
            if expand_form || stk.is_empty() {
                ss.push_str(x);
            }
        };

        let toks = tokenize(ts)?;
        for tok in toks.iter() {
            match tok {
                CMToken::Ident(x) => {
                    if let Some(c) = self.kw.mpsc.get(*x) {
                        present(&grp_stack, &mut ret, *c);
                    } else {
                        present(&grp_stack, &mut ret, self.ident.add_string(x.to_string()));
                    }
                },
                CMToken::Literal(x) => {
                    present(&grp_stack, &mut ret, self.literal.add_string(x.to_string()));
                },
                CMToken::Number(x) => {
                    present_str(&grp_stack, &mut ret, *x);
                },
                CMToken::Punct(x) => {
                    if ! expand_form {
                        if *x == '{' || *x == '(' || *x == '[' {
                            present(&grp_stack, &mut ret, self.group.add_string(x.to_string()));
                            grp_stack.push(*x);
                        } else if *x == '}' {
                            if grp_stack.is_empty() || *grp_stack.get(grp_stack.len() - 1).unwrap() != '{' {
                                return None;
                            }
                            present(&grp_stack, &mut ret, self.group.add_string(x.to_string()));
                            grp_stack.pop();
                        } else if *x == ')' {
                            if grp_stack.is_empty() || *grp_stack.get(grp_stack.len() - 1).unwrap() != '(' {
                                return None;
                            }
                            present(&grp_stack, &mut ret, self.group.add_string(x.to_string()));
                            grp_stack.pop();
                        } else if *x == ']' {
                            if grp_stack.is_empty() || *grp_stack.get(grp_stack.len() - 1).unwrap() != '[' {
                                return None;
                            }
                            present(&grp_stack, &mut ret, self.group.add_string(x.to_string()));
                            grp_stack.pop();
                        } else {
                            present(&grp_stack, &mut ret, self.punct.add_string(x.to_string()));
                        }
                    } else {
                        present(&grp_stack, &mut ret, self.punct.add_string(x.to_string()));
                    }
                },
                CMToken::Ticked(x) => {
                    let ch0 = x.chars().nth(0).unwrap();
                    if ch0 == 'r' { // r leads raw component
                        present_str(&grp_stack, &mut ret, &(*x)[1..]);
                    } else if *x == "iden" {
                        // build regex to match any identifier
                        let rs = self.ident.any_reg();
                        present_str(&grp_stack, &mut ret, &rs);
                    } else {
                        // treat as raw regex component
                        present_str(&grp_stack, &mut ret, *x);
                    }
                },
            }
        };
        Some(ret)
    }

    pub fn find_lit(&self, s: char) -> Option<&String> {
        self.literal.mpcs.get(&s)
    }
    pub fn find_iden(&self, s: char) -> Option<&String> {
        self.ident.mpcs.get(&s)
    }

    pub fn find_ts(&self, s: &str) -> Option<TokenStream> {
        let idx:usize = s.parse().unwrap_or(0x7fffffff);
        if idx < self.vts.len() {
            return Some(self.vts[idx].clone());
        }
        None
    }
    pub fn back_str2<'a, T>(&self, s:T) -> String where T: Iterator<Item=&'a char> {
        let mut ret = String::new();
        for ch0 in s {
            let ch = *ch0;
            if let Some(x) = self.kw.mpcs.get(&ch) {
                ret.push_str(x);
            } else if let Some(x) = self.ident.mpcs.get(&ch) {
                ret.push_str(x);
            } else if let Some(x) = self.number.mpcs.get(&ch) {
                ret.push_str(x);
            } else if let Some(x) = self.literal.mpcs.get(&ch) {
                ret.push_str(x);
            } else if let Some(x) = self.punct.mpcs.get(&ch) {
                ret.push_str(x);
            } else if let Some(x) = self.group.mpcs.get(&ch) {
                ret.push_str(x);
            }
        }
        ret
    }
    pub fn back_str(&self, s:&str) -> String {
        let vec = s.chars().collect::<Vec<char>>();
        return self.back_str2(vec.iter());
    }
}

pub fn test_match(dest: &mut String, reg: &str, cg: &mut Vec<String>) -> bool {
    cg.clear();
    if let Ok(regex) = Regex::new(reg) {
        if let Some(caps) = regex.captures(dest) {
            for i in 1..caps.len() {
                if let Some(a) = caps.get(i) {
                    cg.push(a.as_str().to_string());
                } else {
                    cg.push("".to_string());
                }
            }
            let cap0 = caps.get(0).unwrap().range();
            let d1 = dest.bytes().skip(cap0.end).collect();
            *dest = String::from_utf8(d1).unwrap();
            return true;
        }
    }
    false
}

pub fn test_match2<T>(dest:&mut String, reg: &[T], cg: &mut Vec<String>) -> i32
    where T: AsRef<str>
{
    let mut idx = 0;
    for r in reg {
        if test_match(dest, r.as_ref(), cg) {
            return idx;
        }
        idx += 1;
    }
    -1
}
