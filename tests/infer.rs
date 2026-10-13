use conduct::Codebase;

fn types(src: &str) -> Vec<(String, String)> {
    let mut cb = Codebase::new();
    let added = cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
    added.iter().map(|a| (a.name.clone(), cb.type_of(&a.name).unwrap())).collect()
}

fn ty(src: &str, name: &str) -> String {
    types(src).into_iter().find(|(n, _)| n == name).unwrap().1
}

#[test]
fn infers_basic_functions() {
    assert_eq!(ty("(define (id x) x)", "id"), "(a) -> a");
    assert_eq!(ty("(define (inc x) (+ x 1))", "inc"), "(Int) -> Int");
    assert_eq!(ty("(define (k x y) x)", "k"), "(a, y: Any) -> a");
    assert_eq!(ty("(define (app f x) (f x))", "app"), "((a) -> b, x: a) -> b");
}

#[test]
fn records_are_structural() {
    let claims = "(type name : (t/name ?name)) (type age : (t/age ?age)) (type tags : (t/tags ?tags))";
    assert_eq!(ty(&format!("{claims} (define (name-of r) (name r))"), "name-of"), "({name: a}) -> a");
    assert_eq!(ty(&format!("{claims} (define r {{name: \"x\", age: 3}})"), "r"), "{age: Int, name: String}");
    assert_eq!(
        ty(&format!("{claims} (define (both r) (+ (age r) (count (tags r))))"), "both"),
        "({age: Int, tags: List[Any]}) -> Int"
    );
}

#[test]
fn unions_from_branches_and_defaults() {
    assert_eq!(ty("(define (pick b) (if b 1 \"one\"))", "pick"), "(Bool) -> Int | String");
    // A lookup names what it gives when the key is missing.
    assert_eq!(ty("(define (lookup d) (get :k of: d or: 0))", "lookup"), "(Dict[a]) -> a | Int");
    assert_eq!(ty("(define (head xs) (first xs \"\"))", "head"), "(List[a]) -> a | String");
}

#[test]
fn attributes_may_be_absent() {
    // A record built in two branches may lack an attribute: reading it
    // needs a default, or a `match`.
    let src = "(type id : (t/id ?id)) (type title : (t/title ?title))
               (define (draft t) (if (= t \"\") {id: 1} (assoc {id: 1} title: t)))";
    assert_eq!(ty(src, "draft"), "(a) -> {id: Int, title?: a}");
    let mut cb = Codebase::new();
    cb.add_source(src).unwrap();
    let err = cb.add_source("(define t (title (draft \"x\")))").unwrap_err().to_string();
    assert!(err.contains("has no attribute `t/title`"), "{err}");
    cb.add_source("(define u (title (draft \"x\") \"untitled\"))").unwrap();
    assert_eq!(cb.type_of("u").unwrap(), "String");
    // There is no `nil` to write.
    let err = Codebase::new().add_source("(define n nil)").unwrap_err().to_string();
    assert!(err.contains("conduct has no `nil`"), "{err}");
}

#[test]
fn missing_field_is_an_error() {
    let mut cb = Codebase::new();
    let err = cb
        .add_source("(type a : (t/a ?a)) (type b : (t/b ?b)) (define r {a: 1}) (define x (b r))")
        .unwrap_err()
        .to_string();
    assert!(err.contains("has no attribute `t/b`"), "{err}");
}

#[test]
fn let_is_polymorphic() {
    assert_eq!(
        ty("(type a : (t/a ?a)) (type b : (t/b ?b)) (define p (let [id (fn [x] x)] {a: (id 1), b: (id \"s\")}))", "p"),
        "{a: Int, b: String}"
    );
}

#[test]
fn recursion_and_mutual_recursion() {
    assert_eq!(ty("(define (fact n) (if (< n 1) 1 (* n (fact (- n 1)))))", "fact"), "(Int) -> Int");
    let src = "(define (even? n) (if (= n 0) true (odd? (- n 1))))
               (define (odd? n) (if (= n 0) false (even? (- n 1))))";
    assert_eq!(ty(src, "even?"), "(Int) -> Bool");
    assert_eq!(ty(src, "odd?"), "(Int) -> Bool");
}

#[test]
fn higher_order_builtins() {
    assert_eq!(ty("(define (incs xs) (map xs each: (fn [x] (+ x 1))))", "incs"), "(List[Int]) -> List[Int]");
    assert_eq!(ty("(define mixed [1 \"a\" :x])", "mixed"), "List[Int | String | :x]");
}

#[test]
fn record_update() {
    let claims = "(type a : (t/a ?a)) (type b : (t/b ?b)) (type n : (t/n ?n))";
    assert_eq!(ty(&format!("{claims} (define r (assoc {{a: 1, b: \"x\"}} a: true))"), "r"), "{a: Bool, b: String}");
    // The rest of the record passes through: `a` is whatever else it holds.
    assert_eq!(
        ty(&format!("{claims} (define (bump m) (assoc m n: (+ (n m) 1)))"), "bump"),
        "(a & {n: Int}) -> (a with n: Int)"
    );
    // Reading another field through an update reads the original's.
    assert_eq!(ty(&format!("{claims} (define (f m) (b (assoc m a: 1)))"), "f"), "({b: a}) -> a");
    // Only a record has attributes to set.
    assert_eq!(ty(&format!("{claims} (define (g m) (a (assoc m a: 1)))"), "g"), "({}) -> Int");
    for not_a_record in ["123", "[1 2]", "{:k \"v\"}", "\"text\"", ":kw"] {
        let e = Codebase::new()
            .add_source(&format!("{claims} (define r (assoc {not_a_record} a: 1))"))
            .unwrap_err()
            .to_string();
        assert!(e.contains("`assoc` sets an attribute of a record"), "{not_a_record}: {e}");
    }
    // `{}` is a record with no attributes too.
    assert_eq!(ty(&format!("{claims} (define r (assoc {{}} a: 1))"), "r"), "({} with a: Int)");
}

#[test]
fn bindings_shadow_builtins() {
    assert_eq!(ty("(define (f filter) (count filter))", "f"), "(List[Any]) -> Int");
    assert_eq!(ty("(define (count x) \"mine\") (define y (count [1]))", "y"), "String");
    let mut cb = Codebase::new();
    let err = cb.add_source("(define f map)").unwrap_err().to_string();
    assert!(err.contains("not a value yet"), "{err}");
    let err = cb.add_source("(define g (count [1] [2]))").unwrap_err().to_string();
    assert!(err.contains("takes 1 argument"), "{err}");
}

#[test]
fn keywords_are_their_own_types() {
    assert_eq!(ty("(define k :active)", "k"), ":active");
    assert_eq!(ty("(define (pick b) (if b :active :suspended))", "pick"), "(Bool) -> :active | :suspended");
    assert_eq!(
        ty("(type status : (t/status ?status)) (type id : (t/id ?id)) (define job {status: :active, id: 1})", "job"),
        "{id: Int, status: :active}"
    );
}

#[test]
fn a_match_without_else_is_exhaustive() {
    let label = "(define (label s) (match s :active \"Active\" :suspended \"Paused\"))";
    // The match says what `s` may be.
    assert_eq!(ty(label, "label"), "(:active | :suspended) -> String");
    // A value it leaves out is an error where that value flows in.
    let mut cb = Codebase::new();
    cb.add_source(label).unwrap();
    assert!(cb.add_source("(define a (label :active))").is_ok());
    let err = cb.add_source("(define b (label :paused))").unwrap_err().to_string();
    assert!(err.contains(":paused is not one of :active | :suspended"), "{err}");
    let err = cb.add_source("(define c (label (if true :active :paused)))").unwrap_err().to_string();
    assert!(err.contains(":paused"), "{err}");
    // A record's status flows through the attribute.
    assert_eq!(
        ty(
            "(type status : (t/status ?status)) (type since : (t/since ?since))
             (define (describe job) (match (status job) :active (since job) :suspended \"paused\"))",
            "describe"
        ),
        "({since: a, status: :active | :suspended}) -> a | String"
    );
}

#[test]
fn a_match_with_else_takes_anything() {
    assert_eq!(ty("(define (f s) (match s :a 1 _ 2))", "f"), "(Any) -> Int");
    assert_eq!(ty("(define (g h) (match h \"toggle\" :t \"save\" :s _ :none))", "g"), "(Any) -> :none | :s | :t");
}

#[test]
fn a_match_on_strings_or_integers_must_say_else() {
    let mut cb = Codebase::new();
    let err = cb.add_source("(define (f h) (match h \"toggle\" 1 \"save\" 2))").unwrap_err().to_string();
    assert!(err.contains("cannot cover every value without a `_` clause"), "{err}");
    let err = cb.add_source("(define (g n) (match n 1 :one))").unwrap_err().to_string();
    assert!(err.contains("cannot cover"), "{err}");
    let err = cb.add_source("(define (h k) (match k :a 1 :a 2))").unwrap_err().to_string();
    assert!(err.contains("the same pattern twice"), "{err}");
    let err = cb.add_source("(define (i k) (match k _ 1 :a 2))").unwrap_err().to_string();
    assert!(err.contains("must be the last"), "{err}");
}

#[test]
fn a_match_narrows_what_it_tested() {
    let claims = "(type n : (t/n ?n)) (type id : (t/id ?id))
                  (type status : (t/status ?status)) (type since : (t/since ?since))";
    // Matching an attribute binds it where the record has it.
    let src = format!("{claims} (define (inc job) (match job {{n: n}} (+ n 1) _ 0))");
    assert_eq!(ty(&src, "inc"), "({n?: Int}) -> Int");
    let mut cb = Codebase::new();
    cb.add_source(&src).unwrap();
    assert!(cb.add_source("(define x (inc {id: 1}))").is_ok());
    assert!(cb.add_source("(define y (inc {id: 1, n: \"one\"}))").is_err());
    // A record pattern narrows the matched value; with one attribute
    // matched against keywords in every clause, it needs no `_`.
    let src = format!(
        "{claims} (define (describe job)
                    (match job
                      {{status: :active}} (str \"Since \" (since job))
                      {{status: :suspended}} \"paused\"))"
    );
    assert_eq!(ty(&src, "describe"), "({since: Any, status: :active | :suspended}) -> String");
    let mut cb = Codebase::new();
    cb.add_source(&src).unwrap();
    let err = cb.add_source("(define x (describe {status: :done, since: 1}))").unwrap_err().to_string();
    assert!(err.contains(":done is not one of :active | :suspended"), "{err}");
    // The narrowed value's type is the pattern's, inside its branch.
    let src = "(define (keep s) (match s :a s :b s))";
    assert_eq!(ty(src, "keep"), "(:a | :b) -> :a | :b");
    // A record pattern asks for its attribute only where the value has
    // it, so with a `_` clause the value may be anything.
    let src = format!("{claims} (define (active job) (match job {{status: :active}} (status job) _ :other))");
    assert_eq!(ty(&src, "active"), "(Any) -> :active | :other");
}

#[test]
fn a_match_without_else_must_be_able_to_cover() {
    let mut cb = Codebase::new();
    cb.add_source("(type a : (t/a ?a)) (type b : (t/b ?b))").unwrap();
    // Records of one attribute are variants, so these two cover.
    assert!(cb.add_source("(define (f j) (match j {a: :x} 1 {b: :y} 2))").is_ok());
    let err = cb.add_source("(define (f2 j) (match j {a: \"x\"} 1 {b: :y} 2))").unwrap_err().to_string();
    assert!(err.contains("cannot cover every value without a `_` clause"), "{err}");
    let err = cb.add_source("(define (g j) (match j 1 0 2 1))").unwrap_err().to_string();
    assert!(err.contains("cannot cover"), "{err}");
}

#[test]
fn underscore_matches_anything() {
    // `_` ends a match that cannot otherwise cover.
    assert_eq!(ty("(define (f h) (match h \"add\" 1 _ 0))", "f"), "(Any) -> Int");
    // In a record pattern it asks only that the attribute be there, so a
    // match ending with it covers every record that has the attribute.
    let src = "(type status : (t/status ?status))
               (define (g job) (match job {status: :active} 1 {status: _} 2))";
    assert_eq!(ty(src, "g"), "({status: Any}) -> Int");
    // `:else` is a keyword like any other: here, one case of two.
    assert_eq!(ty("(define (h k) (match k :else 1 :other 2))", "h"), "(:else | :other) -> Int");
}

#[test]
fn variants_grow_through_inference() {
    // A record of one attribute is a variant: unions of them keep every
    // variant, and grow as more branches return more of them.
    let src = "(define (check t) (if (= t \"\") (error :empty-title) (ok t)))
               (define (find id) (if (= id \"\") (error :not-found) (check id)))";
    assert_eq!(ty(src, "check"), "(a) -> (error :empty-title) | (ok a)");
    assert_eq!(ty(src, "find"), "(a) -> (error (:empty-title | :not-found)) | (ok a)");
    // Keywords are the variants that carry nothing.
    assert_eq!(ty("(define (done x) (if x :ok (error \"bad\")))", "done"), "(Bool) -> (error String) | :ok");
    // A variant still reads as the record it is, and setting its own
    // attribute keeps it that variant.
    assert_eq!(ty("(type n : (t/n ?n)) (define v (n {n: 1}))", "v"), "Int");
    let src = "(define (retry r) (match r (ok n) (+ n 1) (error _) 0))
               (define again (retry (assoc (ok 1) conduct.result/ok: 2)))";
    assert_eq!(ty(src, "again"), "Int");
    let mut cb = Codebase::new();
    let err = cb
        .add_source(
            "(type more : (t/more ?more))
             (define (retry r) (match r (ok n) (+ n 1) (error _) 0))
             (define bad (retry (assoc (ok 1) more: 2)))",
        )
        .unwrap_err()
        .to_string();
    assert!(err.contains("has no attribute `conduct.result/ok`") || err.contains("is not one of"), "{err}");
}

#[test]
fn patterns_bind_and_cover_variants() {
    let src = "(define (show r) (match r (ok t) (+ t 1) (error :not-found) 0 (error e) -1))";
    assert_eq!(ty(src, "show"), "((error Any) | (ok Int)) -> Int");
    let mut cb = Codebase::new();
    cb.add_source(src).unwrap();
    cb.add_source("(define a (show (ok 1))) (define b (show (error :gone)))").unwrap();
    // Each variant is checked against its own clause.
    let err = cb.add_source("(define c (show (ok \"one\")))").unwrap_err().to_string();
    assert!(err.contains("String is not a Int"), "{err}");
    // Without `_`, a variant no clause names is a type error where it
    // flows in, as a keyword no clause names is.
    let err = cb.add_source("(type other : (t/other ?other)) (define d (show {other: 1}))").unwrap_err().to_string();
    assert!(err.contains("{t/other: ..} is not one of {conduct.result/error: ..} | {conduct.result/ok: ..}"), "{err}");
    // The keyword `:ok` is not the variant `(ok ..)`.
    let err = cb.add_source("(define e (show :ok))").unwrap_err().to_string();
    assert!(err.contains(":ok is not one of {conduct.result/error: ..} | {conduct.result/ok: ..}"), "{err}");
    // A name binds a payload however deep, and `_` ends an open match.
    let src = "(type todo : (t/todo ?todo)) (type title : (t/title ?title))
               (define (title-of r) (match r (ok {todo: t}) (title t) _ \"none\"))";
    assert_eq!(ty(src, "title-of"), "({ok?: {todo?: {title: a}}}) -> a | String");
    // A pattern may not bind a name twice.
    let err = Codebase::new()
        .add_source("(type a : (t/a ?a)) (type b : (t/b ?b)) (type c : (t/c ?c)) (define (f r) (match r {a: {b: x, c: x}} x _ 0))")
        .unwrap_err()
        .to_string();
    assert!(err.contains("binds `x` twice"), "{err}");
}
