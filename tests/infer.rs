use conduct::Codebase;

fn types(src: &str) -> Vec<(String, String)> {
    let mut cb = Codebase::new();
    let added = cb.add_source(src).unwrap_or_else(|e| panic!("{e}"));
    added.iter().map(|a| (a.name.clone(), cb.defs[&a.hash].type_string())).collect()
}

fn ty(src: &str, name: &str) -> String {
    types(src).into_iter().find(|(n, _)| n == name).unwrap().1
}

#[test]
fn infers_basic_functions() {
    assert_eq!(ty("(defn id [x] x)", "id"), "(a) -> a");
    assert_eq!(ty("(defn inc [x] (+ x 1))", "inc"), "(Int) -> Int");
    assert_eq!(ty("(defn k [x y] x)", "k"), "(a, Any) -> a");
    assert_eq!(ty("(defn app [f x] (f x))", "app"), "((a) => b ! c, a) => b ! c");
}

#[test]
fn records_are_structural() {
    assert_eq!(ty("(defn name [r] (:name r))", "name"), "({name: a}) -> a");
    assert_eq!(ty("(def r {:name \"x\" :age 3})", "r"), "{age: Int, name: String}");
    assert_eq!(ty("(defn both [r] (+ (:age r) (count (:tags r))))", "both"), "({age: Int, tags: List[Any]}) -> Int");
}

#[test]
fn unions_from_branches_and_optionals() {
    assert_eq!(ty("(defn pick [b] (if b 1 \"one\"))", "pick"), "(Bool) -> Int | String");
    assert_eq!(ty("(defn maybe [b] (if b 1 nil))", "maybe"), "(Bool) -> Int | nil");
    assert_eq!(ty("(defn lookup [d] (get d \"k\"))", "lookup"), "(Dict[a]) -> a | nil");
}

#[test]
fn coalescing_removes_nil() {
    // `a` could itself hold nil, so the result is `a` without nil.
    assert_eq!(ty("(defn title [d] (?? (get d \"title\") \"\"))", "title"), "(Dict[a]) -> a \\ nil | String");
    assert_eq!(
        ty("(defn len [d] (if-some [t (get d \"t\")] (+ t 1) 0))", "len"),
        // The dictionary may even hold nil values: `if-some` skips them.
        "(Dict[Int | nil]) -> Int"
    );
    // A plain value passes where an optional is expected.
    assert_eq!(ty("(defn f [x] (?? x 0)) (def y (f 3))", "y"), "Int");
}

#[test]
fn nil_does_not_pass_as_int() {
    let mut cb = Codebase::new();
    let err = cb.add_source("(defn f [d] (+ (get d \"k\") 1))").unwrap_err().to_string();
    assert!(err.contains("nil is not a Int"), "{err}");
}

#[test]
fn missing_field_is_an_error() {
    let mut cb = Codebase::new();
    let err = cb.add_source("(def r {:a 1}) (def b (:b r))").unwrap_err().to_string();
    assert!(err.contains("no field `:b`"), "{err}");
}

#[test]
fn let_is_polymorphic() {
    assert_eq!(ty("(def p (let [id (fn [x] x)] {:a (id 1) :b (id \"s\")}))", "p"), "{a: Int, b: String}");
}

#[test]
fn recursion_and_mutual_recursion() {
    assert_eq!(ty("(defn fact [n] (if (< n 1) 1 (* n (fact (- n 1)))))", "fact"), "(Int) -> Int");
    let src = "(defn even? [n] (if (= n 0) true (odd? (- n 1))))
               (defn odd? [n] (if (= n 0) false (even? (- n 1))))";
    assert_eq!(ty(src, "even?"), "(Int) -> Bool");
    assert_eq!(ty(src, "odd?"), "(Int) -> Bool");
}

#[test]
fn higher_order_builtins() {
    assert_eq!(ty("(defn incs [xs] (map (fn [x] (+ x 1)) xs))", "incs"), "(List[Int]) -> List[Int]");
    assert_eq!(ty("(def mixed [1 \"a\" nil])", "mixed"), "List[Int | String | nil]");
}

#[test]
fn record_update() {
    assert_eq!(ty("(def r (assoc {:a 1 :b \"x\"} :a true))", "r"), "{a: Bool, b: String}");
    // The rest of the record passes through: `a` is whatever else it holds.
    assert_eq!(ty("(defn bump [m] (assoc m :n (+ (:n m) 1)))", "bump"), "(a & {n: Int}) -> (a with n: Int)");
    // Reading another field through an update reads the original's.
    assert_eq!(ty("(defn f [m] (:b (assoc m :a 1)))", "f"), "({b: a}) -> a");
    assert_eq!(ty("(defn g [m] (:a (assoc m :a 1)))", "g"), "(Any) -> Int");
}

#[test]
fn bindings_shadow_builtins() {
    assert_eq!(ty("(defn f [filter] (count filter))", "f"), "(List[Any]) -> Int");
    assert_eq!(ty("(defn count [x] \"mine\") (def y (count [1]))", "y"), "String");
    let mut cb = Codebase::new();
    let err = cb.add_source("(def f map)").unwrap_err().to_string();
    assert!(err.contains("not a value yet"), "{err}");
    let err = cb.add_source("(def g (count [1] [2]))").unwrap_err().to_string();
    assert!(err.contains("takes 1 argument"), "{err}");
}

#[test]
fn keywords_are_their_own_types() {
    assert_eq!(ty("(def k :active)", "k"), ":active");
    assert_eq!(ty("(defn pick [b] (if b :active :suspended))", "pick"), "(Bool) -> :active | :suspended");
    assert_eq!(ty("(def job {:status :active :id 1})", "job"), "{id: Int, status: :active}");
}

#[test]
fn a_match_without_else_is_exhaustive() {
    let label = "(defn label [s] (match s :active \"Active\" :suspended \"Paused\"))";
    // The match says what `s` may be.
    assert_eq!(ty(label, "label"), "(:active | :suspended) -> String");
    // A value it leaves out is an error where that value flows in.
    let mut cb = Codebase::new();
    cb.add_source(label).unwrap();
    assert!(cb.add_source("(def a (label :active))").is_ok());
    let err = cb.add_source("(def b (label :paused))").unwrap_err().to_string();
    assert!(err.contains(":paused is not one of :active | :suspended"), "{err}");
    let err = cb.add_source("(def c (label (if true :active :paused)))").unwrap_err().to_string();
    assert!(err.contains(":paused"), "{err}");
    // A record's status flows through the field.
    assert_eq!(
        ty("(defn describe [job] (match (:status job) :active (:since job) :suspended \"paused\"))", "describe"),
        "({since: a, status: :active | :suspended}) -> a | String"
    );
}

#[test]
fn a_match_with_else_takes_anything() {
    assert_eq!(ty("(defn f [s] (match s :a 1 _ 2))", "f"), "(Any) -> Int");
    assert_eq!(ty("(defn g [h] (match h \"toggle\" :t \"save\" :s _ :none))", "g"), "(Any) -> :none | :s | :t");
}

#[test]
fn a_match_on_strings_or_integers_must_say_else() {
    let mut cb = Codebase::new();
    let err = cb.add_source("(defn f [h] (match h \"toggle\" 1 \"save\" 2))").unwrap_err().to_string();
    assert!(err.contains("cannot cover every value without a `_` clause"), "{err}");
    let err = cb.add_source("(defn g [n] (match n 1 :one))").unwrap_err().to_string();
    assert!(err.contains("cannot cover"), "{err}");
    let err = cb.add_source("(defn h [k] (match k :a 1 :a 2))").unwrap_err().to_string();
    assert!(err.contains("appears twice"), "{err}");
    let err = cb.add_source("(defn i [k] (match k _ 1 :a 2))").unwrap_err().to_string();
    assert!(err.contains("must be the last"), "{err}");
}

#[test]
fn a_match_narrows_what_it_tested() {
    // Matching a field narrows the record it was read from: in the
    // `some?` branch, `(:n job)` is not nil, so `+` takes it.
    let src = "(defn inc [job] (match (:n job) nil 0 some? (+ (:n job) 1)))";
    assert_eq!(ty(src, "inc"), "({n: Int | nil}) -> Int");
    let mut cb = Codebase::new();
    cb.add_source(src).unwrap();
    assert!(cb.add_source("(def x (inc {:n nil}))").is_ok());
    // Without the match, nil reaches `+`.
    cb.add_source("(defn bad [job] (+ (:n job) 1))").unwrap();
    assert!(cb.add_source("(def y (bad {:n nil}))").is_err());
    // A record pattern narrows the matched value; with one field matched
    // against keywords in every clause, it needs no `_`.
    let src = "(defn describe [job]
                 (match job
                   {:status :active} (str \"Since \" (:since job))
                   {:status :suspended} \"paused\"))";
    assert_eq!(ty(src, "describe"), "({since: Any, status: :active | :suspended}) -> String");
    let mut cb = Codebase::new();
    cb.add_source(src).unwrap();
    let err = cb.add_source("(def x (describe {:status :done :since 1}))").unwrap_err().to_string();
    assert!(err.contains(":done is not one of :active | :suspended"), "{err}");
    // The narrowed field's type is the pattern's, inside its branch.
    let src = "(defn keep [s] (match s :a s :b s))";
    assert_eq!(ty(src, "keep"), "(:a | :b) -> :a | :b");
    // A record pattern asks for its field only where the value has it,
    // so with a `_` clause the value may be anything.
    let src = "(defn active [job] (match job {:status :active} (:status job) _ :other))";
    assert_eq!(ty(src, "active"), "(Any) -> :active | :other");
}

#[test]
fn a_match_without_else_must_be_able_to_cover() {
    let mut cb = Codebase::new();
    // Records of one field are variants, so these two cover.
    assert!(cb.add_source("(defn f [j] (match j {:a :x} 1 {:b :y} 2))").is_ok());
    let err = cb.add_source("(defn f2 [j] (match j {:a \"x\"} 1 {:b :y} 2))").unwrap_err().to_string();
    assert!(err.contains("cannot cover every value without a `_` clause"), "{err}");
    let err = cb.add_source("(defn g [j] (match j some? 1))").unwrap_err().to_string();
    assert!(err.contains("cannot cover"), "{err}");
    assert!(cb.add_source("(defn h [j] (match j nil 0 some? 1))").is_ok());
}

#[test]
fn underscore_matches_anything() {
    // `_` ends a match that cannot otherwise cover.
    assert_eq!(ty("(defn f [h] (match h \"add\" 1 _ 0))", "f"), "(Any) -> Int");
    // In a record pattern it asks only that the field be there, so a
    // match ending with it covers every record that has the field.
    let src = "(defn g [job] (match job {:status :active} 1 {:status _} 2))";
    assert_eq!(ty(src, "g"), "({status: Any}) -> Int");
    // `:else` is a keyword like any other: here, one case of two.
    assert_eq!(ty("(defn h [k] (match k :else 1 :other 2))", "h"), "(:else | :other) -> Int");
}

#[test]
fn variants_grow_through_inference() {
    // A record of one field is a variant: unions of them keep every
    // variant, and grow as more branches return more of them.
    let src = "(defn check [t] (if (= t \"\") {:error :empty-title} {:ok t}))
               (defn find [id] (if (= id \"\") {:error :not-found} (check id)))";
    assert_eq!(ty(src, "check"), "(a) -> {error: :empty-title} | {ok: a}");
    assert_eq!(ty(src, "find"), "(a) -> {error: :empty-title | :not-found} | {ok: a}");
    // Keywords are the variants that carry nothing.
    assert_eq!(ty("(defn done [x] (if x :ok {:error \"bad\"}))", "done"), "(Bool) -> {error: String} | :ok");
    // A variant still reads as the record it is, and setting its own
    // field keeps it that variant.
    assert_eq!(ty("(defn n [] (:n {:n 1}))", "n"), "() -> Int");
    let src = "(defn retry [r] (match r {:ok n} (+ n 1) {:error _} 0))
               (def again (retry (assoc {:ok 1} :ok 2)))";
    assert_eq!(ty(src, "again"), "Int");
    let mut cb = Codebase::new();
    let err = cb
        .add_source("(defn retry [r] (match r {:ok n} (+ n 1) {:error _} 0)) (def bad (retry (assoc {:ok 1} :more 2)))")
        .unwrap_err()
        .to_string();
    assert!(err.contains("has no field `:ok`") || err.contains("is not one of"), "{err}");
}

#[test]
fn patterns_bind_and_cover_variants() {
    let src = "(defn show [r] (match r {:ok t} (+ t 1) {:error :not-found} 0 {:error e} -1))";
    assert_eq!(ty(src, "show"), "({error: Any} | {ok: Int}) -> Int");
    let mut cb = Codebase::new();
    cb.add_source(src).unwrap();
    cb.add_source("(def a (show {:ok 1})) (def b (show {:error :gone}))").unwrap();
    // Each variant is checked against its own clause.
    let err = cb.add_source("(def c (show {:ok \"one\"}))").unwrap_err().to_string();
    assert!(err.contains("String is not a Int"), "{err}");
    // Without `_`, a variant no clause names is a type error where it
    // flows in, as a keyword no clause names is.
    let err = cb.add_source("(def d (show {:other 1}))").unwrap_err().to_string();
    assert!(err.contains("{other: ..} is not one of {error: ..} | {ok: ..}"), "{err}");
    let err = cb.add_source("(def e (show :ok))").unwrap_err().to_string();
    assert!(err.contains("the keyword `:ok` is not the variant `{:ok ..}`"), "{err}");
    // A name binds a payload however deep, and `_` ends an open match.
    let src = "(defn title [r] (match r {:ok {:todo t}} (:title t) _ \"none\"))";
    assert_eq!(ty(src, "title"), "({ok?: {todo?: {title: a}}}) -> a | String");
    // A pattern may not bind a name twice.
    let err = Codebase::new().add_source("(defn f [r] (match r {:a {:b x :c x}} x _ 0))").unwrap_err().to_string();
    assert!(err.contains("binds `x` twice"), "{err}");
}
