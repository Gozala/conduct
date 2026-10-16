//! Types are shown as a claim writes them: what is shown, claimed, checks
//! and is shown again the same.

use conduct::types::Ty;
use conduct::{Codebase, element, route};

/// Every definition `src` adds to `base`, claimed with the type it is
/// shown with: each claim must check and show that type again.
fn round_trip(base: fn() -> Codebase, src: &str) {
    round_trip_but(base, src, &[])
}

/// As [`round_trip`], but for names `base` already claims.
fn round_trip_but(base: fn() -> Codebase, src: &str, claimed: &[&str]) {
    let mut cb = base();
    let before: Vec<String> = cb.names.keys().cloned().collect();
    cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
    let names: Vec<String> = cb.names.keys().filter(|n| !before.contains(n)).cloned().collect();
    assert!(!names.is_empty());
    for name in names {
        let base_claims = claimed;
        let claimed = [format!("(claim {name} "), format!("(claim ({name} "), format!("(claim {name}! ")];
        if claimed.iter().any(|c| src.contains(c.as_str())) || base_claims.contains(&name.as_str()) {
            continue;
        }
        let shown = cb.type_of(&name).unwrap();
        let performs = [format!("(define ({name}! "), format!("(define ({name}!)"), format!("(define {name}! ")];
        let bang = if performs.iter().any(|p| src.contains(p.as_str())) { "!" } else { "" };
        let pasted = format!("{src}\n(claim {name}{bang} : {shown})");
        let mut again = base();
        if let Err(e) = again.add_source(&pasted) {
            panic!("`{name}` is shown as `{shown}`, which as a claim fails: {e}");
        }
        assert_eq!(again.type_of(&name).unwrap(), shown, "`{name}` claimed as it is shown");
    }
}

fn core() -> Codebase {
    Codebase::new()
}

fn routes() -> Codebase {
    route::codebase().unwrap()
}

fn printing() -> Codebase {
    let mut cb = Codebase::new();
    cb.add_host("print", "print", vec![Ty::str()], Ty::record([])).unwrap();
    cb
}

#[test]
fn examples() {
    for src in [
        include_str!("../examples/todo-item.cdt"),
        include_str!("../examples/todo-app.cdt"),
        include_str!("../examples/todo-input.cdt"),
        include_str!("../examples/todo-footer.cdt"),
    ] {
        round_trip(element::codebase, src);
    }
    for src in [include_str!("../examples/todos-route.cdt"), include_str!("../examples/todo-route.cdt")] {
        round_trip_but(routes, src, &["handle"]);
    }
}

#[test]
fn tasks() {
    round_trip(
        printing,
        "(define (greet! who) (print! who))
         (define (twice! f x) (do (f! x) (f! x)))
         (define (each! f xs) (match xs [] {} [x rest ...] (do (f! x) (each! f xs: rest))))
         (define (hello!) (print! \"hello\"))",
    );
}

#[test]
fn clauses_and_recursion() {
    round_trip(
        core,
        "(type some : (io.gozala.test/some integer))
         (define (size :none) 0)
         (define (size {some: x}) x)
         (define (len xs) (match xs [] 0 [_ rest ...] (+ 1 (len rest))))
         (define (last xs d) (match xs [] d [x] x [_ rest ...] (last rest d: d)))
         (define (keep xs p) (match xs [] [] [x rest ...] (if (p x) (concat [x] (keep rest p: p)) (keep rest p: p))))
         (define (word true) \"yes\")
         (define (word false) \"no\")",
    );
}

#[test]
fn macros_and_dictionaries() {
    round_trip(
        core,
        "(macro (unless test then: e) (if test {} e))
         (define (f x) (unless (= x 0) then: :zero))
         (define (put d v) (assoc d :k v))
         (define (take d) (dissoc d :k))
         (define (json-of x) (to-json {:x x}))",
    );
}

#[test]
fn functions_lists_and_variants() {
    round_trip(
        core,
        "(define (id x) x)
         (define (twice f x) (f (f x)))
         (define (compose f g) (fn [x] (g (f x))))
         (define (pick b x y) (if b x y))
         (define (first-or xs d) (match xs [x _ ...] x _ d))
         (define (status n) (if (= n 0) :idle :busy))
         (define (wrap x) {:ok x})",
    );
}

#[test]
fn records_open_and_updated() {
    round_trip(
        core,
        "(type n : (io.gozala.test/n integer))
         (type done : (io.gozala.test/done boolean))
         (type title : (io.gozala.test/title text))
         (define (bump r) (assoc r n: (+ (n r) 1)))
         (define (finish r) (assoc r done: true))
         (define (rename r t) (assoc r title: t))
         (define (drop-title {title: t rest ...}) rest)
         (define (maybe-n r) (get n of: r or: 0))
         (define fresh (assoc {} n: 1))",
    );
}

#[test]
fn unions_and_intersections() {
    round_trip(
        core,
        "(define (lookup d) (get :k of: d or: 0))
         (define (by k d) (get k of: d or: \"\"))
         (define (either b x) (if b x 1))
         (define (both f x) (do (f x) x))",
    );
}

#[test]
fn recursive_types() {
    round_trip(
        core,
        "(type next : (io.gozala.test/next any))
         (define (nest x) {next: (nest x)})
         (define (walk r) (walk (next r)))
         (define (depth r) (match r {next: m} (+ 1 (depth m)) _ 0))
         (define (self-apply f) (f f))",
    );
}

fn error(src: &str) -> String {
    Codebase::new().add_source(src).unwrap_err().to_string()
}

#[test]
fn what_is_shown_is_checked() {
    let decl = "(type n : (io.gozala.test/n integer))
                (type title : (io.gozala.test/title text))
                (type next : (io.gozala.test/next any))";
    let e = error(&format!("{decl} (claim (f integer) : [text boolean]) (define (f x) x)"));
    assert!(e.contains("integer is not one of [text boolean]"), "{e}");
    // A record without a field is not the record.
    let e = error(&format!("{decl} (claim (f {{title: text ?a ...}}) : ?a) (define (f {{title: t rest ...}}) rest)"));
    assert!(e.contains("is not ?a"), "{e}");
    // An open record asks for what it lists.
    let e = error(&format!("{decl} (claim (f {{title: text ?a ...}}) : integer) (define (f r) (n r))"));
    assert!(e.contains("io.gozala.test/n"), "{e}");
    let e = error(&format!("{decl} (claim (f (rec ?a {{next: ?a}})) : integer) (define (f r) (+ 1 (next r)))"));
    assert!(e.contains("is not an integer"), "{e}");
    let e = error(&format!("{decl} (claim (f (?a with n: text)) : integer) (define (f r) (n r))"));
    assert!(e.contains("text is not an integer"), "{e}");
    let e = error(&format!("{decl} (claim (f (?a without n)) : integer) (define (f r) (n r))"));
    assert!(e.contains("a pattern took it out"), "{e}");
}

#[test]
fn types_written_as_shown() {
    let decl = "(type n : (io.gozala.test/n integer))
                (type title : (io.gozala.test/title text))";
    let cb = |src: &str| {
        let mut cb = Codebase::new();
        cb.add_source(&format!("{decl} {src}")).unwrap_or_else(|e| panic!("{e}"));
        cb
    };
    let c = cb("(claim (f {n: integer, title?: text}) : integer) (define (f r) (n r))");
    assert_eq!(c.type_of("f").unwrap(), "((\\ {n: integer, title?: text}) : integer)");
    let c = cb("(claim (f (& ?a {n: integer})) : (?a with n: integer)) (define (f r) (assoc r n: (+ (n r) 1)))");
    assert_eq!(c.type_of("f").unwrap(), "((\\ {n: integer ?a ...}) : (?a with n: integer))");
    let c = cb("(claim (f boolean) : [integer text]) (define (f b) (if b 1 \"one\"))");
    assert_eq!(c.type_of("f").unwrap(), "((\\ boolean) : [integer text])");
    let c = cb("(claim (f [:none integer]) : [:none integer]) (define (f x) x)");
    // Shown in one order, however it is written.
    assert_eq!(c.type_of("f").unwrap(), "((\\ [integer :none]) : [integer :none])");
    let e = error(&format!("{decl} (claim (f {{n: integer ?a}}) : ?a) (define (f r) r)"));
    assert!(e.contains("a record type ends with `?a ...`"), "{e}");
    let e = error(&format!("{decl} (claim (f {{size: integer}}) : integer) (define (f r) 1)"));
    assert!(e.contains("`size:` is not a declared attribute"), "{e}");
}
