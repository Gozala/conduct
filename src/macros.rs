//! Macros by rules. A rule has a pattern in its head and a template:
//!
//! ```clojure
//! (macro (cond :else e) e)
//! (macro (cond test then rest ...) (if test then (cond rest ...)))
//! ```
//!
//! Every rule of a name in a compilation, with the rules the codebase
//! already has for it, is one closed set, as a Datalog predicate's rules
//! are: their order means nothing. A call expands by the narrowest rule
//! that matches it, so two rules may overlap only when one is narrower
//! than the other.
//!
//! Hygiene: a symbol the template writes, not a pattern variable, is the
//! macro's own. Each expansion marks it, so it binds only what the same
//! expansion writes and no form of the call can capture it; and it is
//! pinned to the definition it named where the macro was closed, so a
//! later definition of that name does not change what it refers to.

use crate::reader::{MARK, PIN, Sexp, Span, bare};
use crate::term::Hash;
use std::collections::BTreeMap;

/// A pattern for one form.
#[derive(Clone, Debug, PartialEq)]
enum Pat {
    /// `_`: any form.
    Any,
    /// A name: any form, which the template uses by that name.
    Var(String),
    /// A number, string, keyword or label: that form only.
    Lit(Sexp),
    /// A form of forms, `(..)`, `[..]` or `{..}`: patterns for its first
    /// forms and, after `...`, one pattern for any number of the rest.
    Seq(Kind, Vec<Pat>, Option<Box<Pat>>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Parens,
    Brackets,
    Braces,
}

/// What a pattern variable matched: a form, or under `...`, one match
/// per repetition.
#[derive(Clone, Debug)]
enum Bound {
    One(Sexp),
    Many(Vec<Bound>),
}

#[derive(Clone, Debug)]
pub struct Rule {
    /// The pattern for the call's forms after the macro's name.
    pattern: Pat,
    /// Each pattern variable and how many `...` it is under.
    vars: BTreeMap<String, usize>,
    template: Sexp,
    /// The rule's head as written.
    head: String,
    /// The rule as written: what tells two rules apart.
    written: String,
    /// The rule with its template's pins: what its macro's hash covers.
    pub text: String,
}

/// A macro: the closed set of its rules.
#[derive(Clone, Debug)]
pub struct Macro {
    pub rules: Vec<Rule>,
    pub hash: Hash,
}

type Result<T> = std::result::Result<T, String>;

const ELLIPSIS: &str = "...";

fn is_ellipsis(form: &Sexp) -> bool {
    form.as_symbol() == Some(ELLIPSIS)
}

impl Rule {
    /// A rule of the macro `name`: `pattern` is its head's forms after
    /// the name.
    pub fn new(name: &str, pattern: &[Sexp], template: &Sexp) -> Result<Rule> {
        let (fixed, rest) = sequence(pattern)?;
        let pattern = Pat::Seq(Kind::Parens, fixed, rest);
        let mut vars = BTreeMap::new();
        collect(&pattern, 0, &mut vars)?;
        let head = Sexp::List(
            std::iter::once(Sexp::Symbol(name.into(), Span::default()))
                .chain(pattern_forms(pattern_items(&pattern)))
                .collect(),
            Span::default(),
        );
        let head = head.to_string();
        let written = format!("{head} {}", raw(template));
        let rule = Rule { pattern, vars, template: template.clone(), head, text: written.clone(), written };
        rule.check_template(&rule.template, 0)?;
        Ok(rule)
    }

    /// The rule's head, as a person wrote it.
    pub fn head(&self) -> &str {
        &self.head
    }

    /// Every `...` in the template follows a form that uses a variable
    /// matched under one, and every variable is used as deep as it was
    /// matched.
    fn check_template(&self, form: &Sexp, depth: usize) -> Result<()> {
        match form {
            Sexp::Symbol(s, _) if self.vars.contains_key(s.as_str()) => {
                // A variable may be used under more `...` than it matched
                // under, the same each time, but not fewer.
                if self.vars[s.as_str()] > depth {
                    return Err(format!(
                        "`{s}` matched under `...`, so the template uses it under `...` too: `{s} ...`"
                    ));
                }
                Ok(())
            }
            Sexp::List(items, _) | Sexp::Vector(items, _) | Sexp::Map(items, _) => {
                let mut i = 0;
                while i < items.len() {
                    if is_ellipsis(&items[i]) {
                        return Err("`...` follows the template it repeats".into());
                    }
                    let repeated = items.get(i + 1).is_some_and(is_ellipsis);
                    if repeated {
                        let mut used = Vec::new();
                        uses(&items[i], &self.vars, &mut used);
                        if !used.iter().any(|v| self.vars[v] > depth) {
                            return Err(format!(
                                "`{} ...` repeats nothing: it uses no variable the pattern matched under `...`",
                                items[i]
                            ));
                        }
                    }
                    self.check_template(&items[i], depth + repeated as usize)?;
                    i += 1 + repeated as usize;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Pin each symbol the template writes to the definition `resolve`
    /// finds for it, except the name the template itself defines: its
    /// own references to that name are to what it defines.
    pub fn pin(&mut self, resolve: &dyn Fn(&str) -> Option<Hash>) {
        let defines = defined(&self.template);
        let go_resolve =
            |s: &str| if Some(s.strip_suffix('!').unwrap_or(s)) == defines.as_deref() { None } else { resolve(s) };
        fn go(form: &mut Sexp, vars: &BTreeMap<String, usize>, resolve: &dyn Fn(&str) -> Option<Hash>) {
            match form {
                Sexp::Symbol(s, _) if !vars.contains_key(s.as_str()) && !s.contains(PIN) => {
                    if let Some(h) = resolve(s) {
                        s.push(PIN);
                        s.push_str(&h.hex());
                    }
                }
                Sexp::List(items, _) | Sexp::Vector(items, _) | Sexp::Map(items, _) => {
                    items.iter_mut().for_each(|i| go(i, vars, resolve))
                }
                _ => {}
            }
        }
        go(&mut self.template, &self.vars, &go_resolve);
        self.text = format!("{} {}", self.head, raw(&self.template));
    }

    /// The forms of a call, if this rule matches them.
    fn matches(&self, args: &[Sexp]) -> Option<BTreeMap<String, Bound>> {
        let mut out = BTreeMap::new();
        let Pat::Seq(_, fixed, rest) = &self.pattern else { unreachable!() };
        match_seq(fixed, rest.as_deref(), args, &mut out).then_some(out)
    }
}

/// A form with its marks and pins, as the macro's hash covers it.
fn raw(form: &Sexp) -> String {
    let all = |items: &[Sexp]| items.iter().map(raw).collect::<Vec<_>>().join(" ");
    match form {
        Sexp::Symbol(s, _) => s.clone(),
        Sexp::List(items, _) => format!("({})", all(items)),
        Sexp::Vector(items, _) => format!("[{}]", all(items)),
        Sexp::Map(items, _) => format!("{{{}}}", all(items)),
        other => other.to_string(),
    }
}

/// The name a template defines, if it is a definition.
fn defined(template: &Sexp) -> Option<String> {
    let Sexp::List(items, _) = template else { return None };
    match items.as_slice() {
        [head, Sexp::Symbol(name, _), ..] if head.as_symbol() == Some("define") => Some(bare(name).to_string()),
        [head, Sexp::List(sig, _), ..] if head.as_symbol() == Some("define") => {
            let name = bare(sig.first()?.as_symbol()?);
            Some(name.strip_suffix('!').unwrap_or(name).to_string())
        }
        _ => None,
    }
}

/// The patterns of a form of forms: those before `...`, and the one
/// `...` repeats.
fn sequence(forms: &[Sexp]) -> Result<(Vec<Pat>, Option<Box<Pat>>)> {
    let mut fixed = Vec::new();
    let mut i = 0;
    while i < forms.len() {
        if is_ellipsis(&forms[i]) {
            return Err("`...` follows the pattern it repeats".into());
        }
        let p = pattern(&forms[i])?;
        if forms.get(i + 1).is_some_and(is_ellipsis) {
            if i + 2 != forms.len() {
                return Err("`...` repeats the last pattern of a form, so nothing follows it".into());
            }
            return Ok((fixed, Some(Box::new(p))));
        }
        fixed.push(p);
        i += 1;
    }
    Ok((fixed, None))
}

fn pattern(form: &Sexp) -> Result<Pat> {
    Ok(match form {
        Sexp::Symbol(s, _) if s == "_" => Pat::Any,
        Sexp::Symbol(s, _) => {
            if bare(s) != s {
                return Err("a macro's expansion cannot write a macro rule yet".into());
            }
            Pat::Var(s.clone())
        }
        Sexp::List(items, _) => seq(Kind::Parens, items)?,
        Sexp::Vector(items, _) => seq(Kind::Brackets, items)?,
        Sexp::Map(items, _) => seq(Kind::Braces, items)?,
        literal => Pat::Lit(literal.clone()),
    })
}

fn seq(kind: Kind, items: &[Sexp]) -> Result<Pat> {
    let (fixed, rest) = sequence(items)?;
    Ok(Pat::Seq(kind, fixed, rest))
}

fn collect(p: &Pat, depth: usize, out: &mut BTreeMap<String, usize>) -> Result<()> {
    match p {
        Pat::Var(v) => {
            if out.insert(v.clone(), depth).is_some() {
                return Err(format!("the pattern binds `{v}` twice"));
            }
        }
        Pat::Seq(_, fixed, rest) => {
            for f in fixed {
                collect(f, depth, out)?;
            }
            if let Some(r) = rest {
                collect(r, depth + 1, out)?;
            }
        }
        Pat::Any | Pat::Lit(_) => {}
    }
    Ok(())
}

fn uses(form: &Sexp, vars: &BTreeMap<String, usize>, out: &mut Vec<String>) {
    match form {
        Sexp::Symbol(s, _) | Sexp::Label(s, _) if vars.contains_key(s.as_str()) => out.push(s.clone()),
        Sexp::List(items, _) | Sexp::Vector(items, _) | Sexp::Map(items, _) => {
            items.iter().for_each(|i| uses(i, vars, out))
        }
        _ => {}
    }
}

/// A pattern as forms again, for showing a rule.
fn pattern_form(p: &Pat) -> Sexp {
    let span = Span::default();
    match p {
        Pat::Any => Sexp::Symbol("_".into(), span),
        Pat::Var(v) => Sexp::Symbol(v.clone(), span),
        Pat::Lit(l) => l.clone(),
        Pat::Seq(kind, ..) => {
            let items = pattern_forms(pattern_items(p));
            match kind {
                Kind::Parens => Sexp::List(items, span),
                Kind::Brackets => Sexp::Vector(items, span),
                Kind::Braces => Sexp::Map(items, span),
            }
        }
    }
}

fn pattern_items(p: &Pat) -> (&[Pat], Option<&Pat>) {
    match p {
        Pat::Seq(_, fixed, rest) => (fixed, rest.as_deref()),
        _ => (&[], None),
    }
}

fn pattern_forms((fixed, rest): (&[Pat], Option<&Pat>)) -> Vec<Sexp> {
    let mut out: Vec<Sexp> = fixed.iter().map(pattern_form).collect();
    if let Some(r) = rest {
        out.push(pattern_form(r));
        out.push(Sexp::Symbol(ELLIPSIS.into(), Span::default()));
    }
    out
}

fn same_literal(a: &Sexp, b: &Sexp) -> bool {
    match (a, b) {
        (Sexp::Int(x, _), Sexp::Int(y, _)) => x == y,
        (Sexp::Str(x, _), Sexp::Str(y, _))
        | (Sexp::Keyword(x, _), Sexp::Keyword(y, _))
        | (Sexp::Label(x, _), Sexp::Label(y, _)) => x == y,
        _ => false,
    }
}

fn match_one(p: &Pat, form: &Sexp, out: &mut BTreeMap<String, Bound>) -> bool {
    match (p, form) {
        (Pat::Any, _) => true,
        (Pat::Var(v), f) => {
            out.insert(v.clone(), Bound::One(f.clone()));
            true
        }
        (Pat::Lit(l), f) => same_literal(l, f),
        (Pat::Seq(Kind::Parens, fixed, rest), Sexp::List(items, _))
        | (Pat::Seq(Kind::Brackets, fixed, rest), Sexp::Vector(items, _))
        | (Pat::Seq(Kind::Braces, fixed, rest), Sexp::Map(items, _)) => match_seq(fixed, rest.as_deref(), items, out),
        _ => false,
    }
}

fn match_seq(fixed: &[Pat], rest: Option<&Pat>, forms: &[Sexp], out: &mut BTreeMap<String, Bound>) -> bool {
    if forms.len() < fixed.len() || (rest.is_none() && forms.len() != fixed.len()) {
        return false;
    }
    if !fixed.iter().zip(forms).all(|(p, f)| match_one(p, f, out)) {
        return false;
    }
    let Some(rest) = rest else { return true };
    let mut vars = BTreeMap::new();
    let _ = collect(rest, 0, &mut vars);
    let mut each: BTreeMap<String, Vec<Bound>> = vars.keys().map(|v| (v.clone(), Vec::new())).collect();
    for form in &forms[fixed.len()..] {
        let mut one = BTreeMap::new();
        if !match_one(rest, form, &mut one) {
            return false;
        }
        for (v, b) in one {
            each.get_mut(&v).unwrap().push(b);
        }
    }
    out.extend(each.into_iter().map(|(v, bs)| (v, Bound::Many(bs))));
    true
}

/// Whether every form `a` matches, `b` matches too.
fn narrower(a: &Pat, b: &Pat) -> bool {
    match (a, b) {
        (_, Pat::Any | Pat::Var(_)) => true,
        (Pat::Any | Pat::Var(_), _) => false,
        (Pat::Lit(x), Pat::Lit(y)) => same_literal(x, y),
        (Pat::Seq(k, af, ar), Pat::Seq(l, bf, br)) if k == l => {
            let at = |i: usize| af.get(i).or(ar.as_deref());
            let bt = |i: usize| bf.get(i).or(br.as_deref());
            match (ar, br) {
                // `a`'s lengths are af.len() and up: `b` takes them all
                // only with a repetition of its own, after no more fixed
                // forms than `a` has.
                (Some(ar), Some(br)) => {
                    bf.len() <= af.len() && (0..af.len()).all(|i| narrower(&af[i], bt(i).unwrap())) && narrower(ar, br)
                }
                (Some(_), None) => false,
                (None, _) => {
                    let n = af.len();
                    (br.is_some() && bf.len() <= n || bf.len() == n)
                        && (0..n).all(|i| narrower(at(i).unwrap(), bt(i).unwrap()))
                }
            }
        }
        _ => false,
    }
}

/// Whether some form matches both.
fn overlap(a: &Pat, b: &Pat) -> bool {
    match (a, b) {
        (Pat::Any | Pat::Var(_), _) | (_, Pat::Any | Pat::Var(_)) => true,
        (Pat::Lit(x), Pat::Lit(y)) => same_literal(x, y),
        (Pat::Seq(k, af, ar), Pat::Seq(l, bf, br)) if k == l => {
            // The shortest length both take is the one to try: a longer
            // one only adds positions that must overlap too.
            let n = match (ar, br) {
                (None, None) if af.len() != bf.len() => return false,
                (None, Some(_)) if af.len() < bf.len() => return false,
                (Some(_), None) if bf.len() < af.len() => return false,
                (None, _) => af.len(),
                (_, None) => bf.len(),
                _ => af.len().max(bf.len()),
            };
            let at = |i: usize| af.get(i).or(ar.as_deref());
            let bt = |i: usize| bf.get(i).or(br.as_deref());
            (0..n).all(|i| overlap(at(i).unwrap(), bt(i).unwrap()))
        }
        _ => false,
    }
}

impl Macro {
    /// Close a set of rules: any two that overlap must be ordered, one
    /// narrower than the other.
    pub fn close(name: &str, mut rules: Vec<Rule>) -> Result<Macro> {
        // A rule added again is the same rule, pinned or not; the one
        // added first, pinned already, is kept.
        rules.sort_by(|a, b| a.written.cmp(&b.written));
        rules.dedup_by(|a, b| a.written == b.written);
        for (i, a) in rules.iter().enumerate() {
            for b in &rules[i + 1..] {
                if !overlap(&a.pattern, &b.pattern) {
                    continue;
                }
                match (narrower(&a.pattern, &b.pattern), narrower(&b.pattern, &a.pattern)) {
                    (true, true) => {
                        return Err(format!(
                            "`{}` and `{}` match the same calls, so a call could expand by either",
                            a.head(),
                            b.head()
                        ));
                    }
                    (false, false) => {
                        return Err(format!(
                            "some call matches both `{}` and `{}`, and neither is narrower, so it could expand \
                             by either: make one narrower than the other, or make them disjoint",
                            a.head(),
                            b.head()
                        ));
                    }
                    _ => {}
                }
            }
        }
        let text: Vec<&str> = rules.iter().map(|r| r.text.as_str()).collect();
        let hash = Hash::of(format!("macro {name}\n{}", text.join("\n")).as_bytes());
        Ok(Macro { rules, hash })
    }

    /// The form a call of `name` with `args` stands for, its template's
    /// own symbols marked with `mark`.
    pub fn expand(&self, name: &str, args: &[Sexp], mark: u64, span: Span) -> Result<Sexp> {
        let mut best: Option<(&Rule, BTreeMap<String, Bound>)> = None;
        for rule in &self.rules {
            if let Some(bound) = rule.matches(args)
                && best.as_ref().is_none_or(|(b, _)| narrower(&rule.pattern, &b.pattern))
            {
                best = Some((rule, bound));
            }
        }
        let Some((rule, bound)) = best else {
            let call = Sexp::List(
                std::iter::once(Sexp::Symbol(name.into(), span)).chain(args.iter().cloned()).collect(),
                span,
            );
            let heads: Vec<String> = self.rules.iter().map(|r| format!("`{}`", r.head())).collect();
            return Err(format!("no rule of `{name}` matches `{call}`; its rules are {}", heads.join(", ")));
        };
        substitute(&rule.template, &bound, mark, span)
    }
}

fn substitute(form: &Sexp, bound: &BTreeMap<String, Bound>, mark: u64, span: Span) -> Result<Sexp> {
    Ok(match form {
        Sexp::Symbol(s, _) => match bound.get(s.as_str()) {
            Some(Bound::One(f)) => f.clone(),
            Some(Bound::Many(_)) => return Err(format!("`{s}` matched under `...`, so it is used under `...`")),
            None => {
                let (name, pin) = match s.split_once(PIN) {
                    Some((name, hex)) => (name, format!("{PIN}{hex}")),
                    None => (s.as_str(), String::new()),
                };
                Sexp::Symbol(format!("{name}{MARK}{mark}{pin}"), span)
            }
        },
        Sexp::List(items, _) => Sexp::List(substitute_all(items, bound, mark, span)?, span),
        Sexp::Vector(items, _) => Sexp::Vector(substitute_all(items, bound, mark, span)?, span),
        Sexp::Map(items, _) => Sexp::Map(substitute_all(items, bound, mark, span)?, span),
        Sexp::Int(n, _) => Sexp::Int(*n, span),
        Sexp::Str(s, _) => Sexp::Str(s.clone(), span),
        Sexp::Keyword(k, _) => Sexp::Keyword(k.clone(), span),
        // A label named by a variable bound to a symbol is that symbol's
        // label, so a template can write the attribute or parameter a call
        // names: `(assoc r name: v)` with `name` bound to `title` writes
        // `title:`.
        Sexp::Label(l, _) => match bound.get(l.as_str()) {
            Some(Bound::One(Sexp::Symbol(s, _))) => Sexp::Label(s.clone(), span),
            Some(Bound::One(other)) => {
                return Err(format!("`{l}:` names a label, so `{l}` must be a name, not `{other}`"));
            }
            Some(Bound::Many(_)) => return Err(format!("`{l}` matched under `...`, so it is used under `...`")),
            None => Sexp::Label(l.clone(), span),
        },
    })
}

fn substitute_all(items: &[Sexp], bound: &BTreeMap<String, Bound>, mark: u64, span: Span) -> Result<Vec<Sexp>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < items.len() {
        // `...` repeats the form before it, or a label and the form after
        // it together, as a record's `{name: value ...}` needs.
        let width = if matches!(items[i], Sexp::Label(..)) && items.get(i + 2).is_some_and(is_ellipsis) {
            2
        } else if items.get(i + 1).is_some_and(is_ellipsis) {
            1
        } else {
            0
        };
        if width > 0 {
            // Repeat the forms once per match of the variables they use
            // that were matched under `...`.
            let group = &items[i..i + width];
            let mut used = Vec::new();
            let vars = bound.keys().map(|k| (k.clone(), 0)).collect();
            group.iter().for_each(|f| uses(f, &vars, &mut used));
            let repeated: Vec<(&String, &Vec<Bound>)> = used
                .iter()
                .filter_map(|v| match &bound[v] {
                    Bound::Many(bs) => Some((v, bs)),
                    Bound::One(_) => None,
                })
                .collect();
            let n = repeated.first().map(|(_, bs)| bs.len()).unwrap_or(0);
            if let Some((v, _)) = repeated.iter().find(|(_, bs)| bs.len() != n) {
                return Err(format!("`{v}` and `{}` repeat a different number of times", repeated[0].0));
            }
            for k in 0..n {
                let mut each = bound.clone();
                for (v, bs) in &repeated {
                    each.insert((*v).clone(), bs[k].clone());
                }
                for f in group {
                    out.push(substitute(f, &each, mark, span)?);
                }
            }
            i += width + 1;
        } else {
            out.push(substitute(&items[i], bound, mark, span)?);
            i += 1;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::read;

    fn rule(src: &str) -> Rule {
        let forms = read(src).unwrap();
        let Sexp::List(items, _) = &forms[0] else { panic!() };
        let Sexp::List(head, _) = &items[1] else { panic!() };
        Rule::new(head[0].as_symbol().unwrap(), &head[1..], &items[2]).unwrap()
    }

    #[test]
    fn narrower_rules_are_found() {
        let else_ = rule("(macro (cond :else e) e)");
        let test = rule("(macro (cond test then rest ...) (if test then (cond rest ...)))");
        assert!(narrower(&else_.pattern, &test.pattern));
        assert!(!narrower(&test.pattern, &else_.pattern));
        assert!(overlap(&else_.pattern, &test.pattern));
        let one = rule("(macro (and x) x)");
        let more = rule("(macro (and x rest ...) (if x (and rest ...) false))");
        assert!(narrower(&one.pattern, &more.pattern));
        let two = rule("(macro (f a b) a)");
        let lits = rule("(macro (f :x b) b)");
        let other = rule("(macro (f a :y) a)");
        // `(f :x :y)` matches both, and neither contains the other.
        assert!(overlap(&lits.pattern, &other.pattern));
        assert!(!narrower(&lits.pattern, &other.pattern) && !narrower(&other.pattern, &lits.pattern));
        assert!(narrower(&lits.pattern, &two.pattern));
        assert!(!overlap(&rule("(macro (g [a]) a)").pattern, &rule("(macro (g (a)) a)").pattern));
    }
}
