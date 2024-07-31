#!/usr/bin/env python3
# encoding:utf-8

# NOTE: this is NOT a full functional converter, it is designed to only support to convert
# some format of rust structures into C++ struct while keeping bit by bit compatiable.
# Use it at your OWN RISK and you can extend it as you need.
# the #[repr(C)] attribute is required to make a rust struct being able to be compatiable with C struct.
import re
import sys
import argparse
import io


class MyParser:
    def __init__(self, instr, rules, token_func):
        self.instr = instr
        self.con_str = ""
        self.kw_map = dict()
        self.op_map = dict()
        # https://en.wikipedia.org/wiki/Unicode_block
        self.kw_start = 0x2460
        self.kw_end = 0x257f
        self.op_start = 0x2200
        self.op_end = 0x22ff
        self.rules = []
        self.op_map[';'] = ';'
        self.tokens = []
        beg1 = 0
        while True:
            name, ss, len1 = token_func(instr)
            if name == "EOF":
                break
            if name in ['SPACE', "NEWLINE", "COMMENT", "COMMENT1", "COMMENT2", "COMMENT3"]:
                beg1 += len1
                instr = instr[len1:]
                continue
            if name == "KEYWORD":
                self.con_str += self._char4kw(ss)
            elif name == "OPERATOR":
                self.con_str += self._char4op(ss)
            elif name == "IDEN" or name == "IDENTIFIER":
                self.con_str += 'I'
            elif name == "NUMBER":
                self.con_str += 'N'
            elif name == "STRING":
                self.con_str += 'S'
            elif name == "STRING1":
                self.con_str += '\u2192'
            elif name == "STRING2":
                self.con_str += '\u21d2'
            else:
                raise RuntimeError(f"unknown token: {name}")
            self.tokens.append((name, ss, beg1, len1))
            beg1 += len1
            instr = instr[len1:]
        # load rules.
        rl_index = 0
        for rl in rules:
            if isinstance(rl, list):
                rl_meta = rl[1:]
                rl = rl[0]
            else:
                rl_meta = rl_index
            assert isinstance(rl, str)
            rl_index += 1
            self._parse_rule(rl, rl_meta)
        self.all_keywords = "".join(self.kw_map.values())
        self.all_operators = "".join(self.op_map.values())

    def _parse_rule(self, rl, rl_meta):
        regex = ""
        while rl:
            if m := re.match(r"(?:KW|KEYWORD)\{(\w+)}", rl):
                regex += self._char4kw(m.group(1))
                rl = rl[m.end():]
            elif m := re.match(r"(?:OP|OPERATOR)\{(.*?)}", rl):
                regex += self._char4op(m.group(1))
                rl = rl[m.end():]
            elif m := re.match(r"(?:OP|OPERATOR)<(.*?)>", rl):
                regex += self._char4op(m.group(1))
                rl = rl[m.end():]
            elif m := re.match(r"IDEN(?:TIFIER)?", rl):
                regex += 'I'
                rl = rl[m.end():]
            elif m := re.match(r"NUM(?:BER)?", rl):
                regex += 'N'
                rl = rl[m.end():]
            elif m := re.match(r"STR(?:ING)?(\d?)", rl):
                if m.group(1) == "1":
                    regex += '\u2192'
                elif m.group(1) == "2":
                    regex += '\u21d2'
                elif m.group(1) == "":
                    regex += 'S'
                else:
                    raise RuntimeError("unknown string type")
                rl = rl[m.end():]
            elif m := re.match(r"\s+", rl):
                rl = rl[m.end():]
            elif m := re.match(r"[(){}\[\]|+\-?*:0-9,]", rl):
                regex += m.group()
                rl = rl[m.end():]
            else:
                raise RuntimeError(f"unknown token: {rl}")
        self.rules.append((re.compile(regex), rl_meta))

    def _char4kw(self, ss):
        if ss in self.kw_map:
            return self.kw_map.get(ss)
        if self.kw_start > self.kw_end:
            raise RuntimeError("too many keywords")
        next_kw = chr(self.kw_start)
        self.kw_start += 1
        self.kw_map[ss] = next_kw
        return next_kw

    def _char4op(self, ss):
        if ss in self.op_map:
            return self.op_map.get(ss)
        if self.op_start > self.op_end:
            raise RuntimeError("too many operators")
        next_op = chr(self.op_start)
        self.op_start += 1
        self.op_map[ss] = next_op
        return next_op

    def _back2str(self, pre_tokens, matched):
        prev_itm = ' '
        rets = ""
        # if prev_itm and itm both in [keywords, identifiers, number, string], we need a space between them.
        need_space = self.all_keywords + "INS\u2192\u21d2"
        for itm in matched:
            _, ss, beg1, len1 = self.tokens[pre_tokens]
            if prev_itm in need_space and itm in need_space:
                rets += ' '
            rets += ss
            pre_tokens += 1
            prev_itm = itm
        return rets

    def _back2strs(self, precnt, m, ng):
        retss = []
        for i in range(ng):
            if m.start(i+1) == -1:
                retss.append(None)
            else:
                retss.append(self._back2str(precnt + m.start(i+1), m.group(i+1)))
        return retss

    def parse(self):
        pre_tokens = 0
        while self.con_str:
            for regex, rl_meta in self.rules:
                if m := regex.match(self.con_str):
                    yield rl_meta, self._back2str(pre_tokens, m.group()), self._back2strs(pre_tokens, m, regex.groups)
                    pre_tokens += len(m.group())
                    self.con_str = self.con_str[m.end():]
                    break
            else:
                ss1 = self._back2str(pre_tokens, self.con_str[0:10])
                raise RuntimeError(f"no rule matched: {ss1}")


class Rust2H:
    @staticmethod
    def get_token(instr):
        if not instr:
            return "EOF", "", 0
        if instr[0] == '\ufeff':
            return "SPACE", instr[0], 1
        if instr[0].isspace():
            i = 1
            while i < len(instr) and instr[i].isspace():
                i += 1
            return "SPACE", instr[:i], i
        if instr.startswith("/*"):
            i = instr.find("*/")
            if i == -1:
                raise Exception("comment not closed")
            return "COMMENT", instr[:i + 2], i + 2
        if instr.startswith("//"):
            i = instr.find("\n")
            if i == -1:
                return "COMMENT", instr, len(instr)
            return "COMMENT", instr[:i], i
        if instr.startswith('"'):
            i = 1
            while i < len(instr) and instr[i] != '"':
                if instr[i] == "\\":
                    i += 2
                else:
                    i += 1
            if i == len(instr):
                raise Exception("string not closed")
            return "STRING2", instr[:i + 1], i + 1
        if instr.startswith("'"):
            i = 1
            while i < len(instr) and instr[i] != "'":
                if instr[i] == "\\":
                    i += 2
                else:
                    i += 1
            if i == len(instr):
                raise Exception("char not closed")
            return "STRING1", instr[:i + 1], i + 1
        if len(instr) > 2 and instr[:2] == "r#":
            i = 2
            while i < len(instr) and instr[i].isalnum():
                i += 1
            return "IDEN", instr[2:i], i
        operators_2 = ("!=", "%=", "&&", "&=", "*=", "+=", "-=", "..", "/=", "<<", "<=", "==", ">=", ">>", "^=", "|=", "||",)
        operators_2b = ("::",)
        if len(instr) >= 3 and instr[:3] in ("..=", "<<=", ">>="):
            return "OPERATOR", instr[:3], 3
        elif len(instr) >= 2 and instr[0:2] in operators_2:
            return "OPERATOR", instr[:2], 2
        elif len(instr) >= 2 and instr[0:2] in operators_2b:
            return "OPERATOR", instr[:2], 2
        elif instr[0] in "!%&*+-/<=>?^|{}[](),.:;#":
            return "OPERATOR", instr[0], 1
        kwds = ["as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in",
                "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct",
                "super", "trait", "true", "type", "unsafe", "use", "where", "while", "async", "await", "dyn"]
        if instr[0].isalpha() or instr[0] == "_":
            i = 1
            while i < len(instr) and (instr[i].isalnum() or instr[i] == "_"):
                i += 1
            if instr[:i] in kwds:
                return "KEYWORD", instr[:i], i
            return "IDEN", instr[:i], i
        if instr[0].isdigit():
            i = 1
            while i < len(instr) and instr[i].isalnum():
                i += 1
            return "NUMBER", instr[:i], i
        raise Exception(f"unknown token: {instr}")

    def __init__(self, infile, outfile):
        self.outf = sys.stdout
        self.inf = sys.stdin
        self.outfile = outfile
        if infile:
            self.inf = open(infile, "r", encoding="utf-8")
        if outfile:
            self.outf = io.StringIO()

    def _trans_type(self, ss, has_err):
        maping_tbl = { "String": "RustString", "u32": "uint32_t", "i32":"int", "u64": "uint64_t", "i64": "int64_t", "usize": "size_t",
                       "isize": "ssize_t", "f32": "float", "f64": "double", "u8": "uint8_t", "i8": "int8_t", "u16": "uint16_t",
                       "i16": "int16_t", "byte": "uint8_t"}
        if m := re.match(r"Vec<(.+)>", ss):
            return f"RustVec<{self._trans_type(m.group(1), has_err)}>"
        if m := re.match(r"Option<(.+)>", ss):
            return f"RustOption<{self._trans_type(m.group(1), has_err)}>"
        if ss in maping_tbl:
            return maping_tbl.get(ss)
        if '<' in ss or "::" in ss:
            has_err[0] = 1
            return f"<ERROR_TYPE({ss})>"
        return ss

    def translate(self):
        rules = [
            # struct meta like #[derive(Serialize, Deserialize, Debug, Clone)]
            "OP{#} OP{[} IDEN OP{(} (IDEN (?:OP{=}[STR2|NUM])?  OP{,})* IDEN (?:OP{=}[STR2|NUM])? OP{)} OP{]}",
            # start a pub? struct.
            "(KW{pub})? KW{struct} (IDEN) OP<{>",
            # struct field
            "(KW{pub})? (IDEN) OP{:} ((?:IDEN OP{::})*IDEN (OP{<} (?:IDEN OP{::})* IDEN OP{>})?) OP{,}?",
            # struct end
            "OP<}> OP{;}?",
            "KW{use} (?: OP{::} KW{crate} OP{::})? (?:IDEN OP{::})* IDEN OP{;}",
            "KW{use} (?: OP{::} KW{crate} OP{::})? (?:IDEN OP{::})* OP<{> (IDEN OP{,})* IDEN  OP<}> OP{;}",
        ]
        instr = self.inf.read()
        p = MyParser(instr, rules, Rust2H.get_token)
        print("#pragma once", file=self.outf)
        print("#include \"rust/rust-common.h\"\n", file=self.outf)

        ST_INIT, ST_STRUCT, ST_END = 0, 1, 2
        state = ST_INIT
        attrs = []
        should_trans = False
        current_st = ""
        has_err = [0]
        for idx,ss,grps in p.parse():
            if idx >= 4:
                continue
            if idx == 0 and state == ST_INIT:
                attrs.append(ss)
            elif idx == 1 and state == ST_INIT:
                should_trans = grps[0] == "pub" and any(["repr(C)" in x for x in attrs])
                current_st += f"struct {grps[1]} {{\n"
                state = ST_STRUCT
                if not should_trans:
                    print(f"Skip struct {grps[1]} since it's not repr(C)", file=sys.stderr)
            elif idx == 2 and state == ST_STRUCT:
                current_st += "\t"
                current_st += self._trans_type(grps[2], has_err)
                current_st += f" {grps[1]};\n"
            elif idx == 3 and state == ST_STRUCT:
                current_st += "};\n"
                state = ST_INIT
                attrs = []
                if should_trans:
                    if has_err[0]:
                        print(f"Warning: the struct has unknown type of fields, please fixit and retry:\n{current_st}", file=sys.stderr)
                    else:
                        print(current_st, file=self.outf)
                current_st = ""
                has_err = [0]
        self.flush()

    def flush(self):
        if self.outfile:
            changed = True
            try:
                with open(self.outfile, "r", encoding="utf-8") as f:
                    changed = f.read() != self.outf.getvalue()
            except FileNotFoundError:
                pass
            if changed:
                with open(self.outfile, "w", encoding="utf-8") as f:
                    f.write(self.outf.getvalue())
            else:
                print(f"No changes for file {self.outfile}", file=sys.stderr)
        self.outf.close()
        self.inf.close()


if __name__ == "__main__":
    def main():
        ap = argparse.ArgumentParser()
        ap.add_argument('-o', dest="outfile", help="Output file")
        ap.add_argument("infile", help="Input file")
        args = ap.parse_args()
        Rust2H(args.infile, args.outfile).translate()


    main()
