  ;; The runtime every compiled module carries. Spliced into the module
  ;; that `codegen` writes, which defines the memory, the function
  ;; table `$fns`, the function types `$fn0`.., the heap pointer `$hp`,
  ;; and the `$applyN` helpers.
  ;;
  ;; A value is an i64:
  ;;   ...xx1  an integer n, as (n << 1) | 1
  ;;   0x02    nil
  ;;   0x0a    false
  ;;   0x12    true
  ;;   ...000  a pointer to a heap object, 8-aligned and non-zero
  ;;
  ;; A heap object starts with two i32s, its kind and a length:
  ;;   1 string   length in bytes, then the UTF-8 bytes
  ;;   2 list     length, then length i64 values
  ;;   3 record   field count, then (field id, value) i64 pairs, sorted by id
  ;;   4 dict     entry count, then (key string, value) i64 pairs
  ;;   5 closure  free variable count, then i32 table index, i32 arity,
  ;;              then the free variables
  ;;   6 view     4, then tag, attributes, children, events
  ;;   7 emit     2, then name, detail
  ;;
  ;; Memory is a bump arena: nothing is freed in this iteration.

  (func $alloc (param $n i32) (result i32)
    (local $p i32) (local $end i32) (local $need i32)
    (local.set $p (global.get $hp))
    (local.set $end (i32.and (i32.add (i32.add (local.get $p) (local.get $n)) (i32.const 7)) (i32.const -8)))
    (if (i32.gt_u (local.get $end) (i32.mul (memory.size) (i32.const 65536)))
      (then
        (local.set $need (i32.sub
          (i32.shr_u (i32.add (local.get $end) (i32.const 65535)) (i32.const 16))
          (memory.size)))
        (if (i32.eq (memory.grow (local.get $need)) (i32.const -1)) (then unreachable))))
    (global.set $hp (local.get $end))
    (local.get $p))

  (func $obj (param $kind i32) (param $len i32) (param $bytes i32) (result i32)
    (local $p i32)
    (local.set $p (call $alloc (i32.add (local.get $bytes) (i32.const 8))))
    (i32.store (local.get $p) (local.get $kind))
    (i32.store offset=4 (local.get $p) (local.get $len))
    (local.get $p))

  (func $ptr (param $v i64) (result i32) (i32.wrap_i64 (local.get $v)))
  (func $is_obj (param $v i64) (result i32)
    (i32.and (i64.eqz (i64.and (local.get $v) (i64.const 7))) (i64.ne (local.get $v) (i64.const 0))))
  (func $kind (param $v i64) (result i32)
    (if (result i32) (call $is_obj (local.get $v))
      (then (i32.load (call $ptr (local.get $v))))
      (else (i32.const 0))))
  (func $len (param $v i64) (result i32) (i32.load offset=4 (call $ptr (local.get $v))))
  (func $bool (param $b i32) (result i64)
    (select (i64.const 0x12) (i64.const 0x0a) (local.get $b)))

  ;; Integers
  (func $int_add (param $a i64) (param $b i64) (result i64)
    (i64.sub (i64.add (local.get $a) (local.get $b)) (i64.const 1)))
  (func $int_sub (param $a i64) (param $b i64) (result i64)
    (i64.add (i64.sub (local.get $a) (local.get $b)) (i64.const 1)))
  (func $int_mul (param $a i64) (param $b i64) (result i64)
    (i64.add (i64.mul (i64.shr_s (local.get $a) (i64.const 1)) (i64.sub (local.get $b) (i64.const 1))) (i64.const 1)))
  (func $int_quot (param $a i64) (param $b i64) (result i64)
    (i64.or (i64.shl (i64.div_s (i64.shr_s (local.get $a) (i64.const 1)) (i64.shr_s (local.get $b) (i64.const 1))) (i64.const 1)) (i64.const 1)))
  (func $int_rem (param $a i64) (param $b i64) (result i64)
    (i64.or (i64.shl (i64.rem_s (i64.shr_s (local.get $a) (i64.const 1)) (i64.shr_s (local.get $b) (i64.const 1))) (i64.const 1)) (i64.const 1)))

  ;; Strings
  (func $str_new (param $src i32) (param $len i32) (result i64)
    (local $p i32)
    (local.set $p (call $obj (i32.const 1) (local.get $len) (local.get $len)))
    (memory.copy (i32.add (local.get $p) (i32.const 8)) (local.get $src) (local.get $len))
    (i64.extend_i32_u (local.get $p)))
  (func $str_data (param $v i64) (result i32) (i32.add (call $ptr (local.get $v)) (i32.const 8)))
  (func $bytes_eq (param $a i32) (param $b i32) (param $n i32) (result i32)
    (local $i i32)
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (if (i32.ne (i32.load8_u (i32.add (local.get $a) (local.get $i)))
                    (i32.load8_u (i32.add (local.get $b) (local.get $i))))
          (then (return (i32.const 0))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (i32.const 1))
  (func $str_eq (param $a i64) (param $b i64) (result i32)
    (if (result i32) (i32.ne (call $len (local.get $a)) (call $len (local.get $b)))
      (then (i32.const 0))
      (else (call $bytes_eq (call $str_data (local.get $a)) (call $str_data (local.get $b)) (call $len (local.get $a))))))
  (func $str_concat (param $a i64) (param $b i64) (result i64)
    (local $p i32) (local $la i32) (local $lb i32)
    (local.set $la (call $len (local.get $a)))
    (local.set $lb (call $len (local.get $b)))
    (local.set $p (call $obj (i32.const 1) (i32.add (local.get $la) (local.get $lb)) (i32.add (local.get $la) (local.get $lb))))
    (memory.copy (i32.add (local.get $p) (i32.const 8)) (call $str_data (local.get $a)) (local.get $la))
    (memory.copy (i32.add (i32.add (local.get $p) (i32.const 8)) (local.get $la)) (call $str_data (local.get $b)) (local.get $lb))
    (i64.extend_i32_u (local.get $p)))
  (func $int_to_str (param $v i64) (result i64)
    ;; Digits are written backwards into a 24-byte scratch buffer.
    (local $n i64) (local $neg i32) (local $pos i32) (local $buf i32)
    (local.set $buf (call $alloc (i32.const 24)))
    (local.set $pos (i32.add (local.get $buf) (i32.const 24)))
    (local.set $n (i64.shr_s (local.get $v) (i64.const 1)))
    (if (i64.lt_s (local.get $n) (i64.const 0))
      (then (local.set $neg (i32.const 1)) (local.set $n (i64.sub (i64.const 0) (local.get $n)))))
    (loop $digit
      (local.set $pos (i32.sub (local.get $pos) (i32.const 1)))
      (i32.store8 (local.get $pos) (i32.wrap_i64 (i64.add (i64.rem_u (local.get $n) (i64.const 10)) (i64.const 48))))
      (local.set $n (i64.div_u (local.get $n) (i64.const 10)))
      (br_if $digit (i64.ne (local.get $n) (i64.const 0))))
    (if (local.get $neg)
      (then
        (local.set $pos (i32.sub (local.get $pos) (i32.const 1)))
        (i32.store8 (local.get $pos) (i32.const 45))))
    (call $str_new (local.get $pos) (i32.sub (i32.add (local.get $buf) (i32.const 24)) (local.get $pos))))
  (func $lit (param $offset i32) (param $len i32) (result i64)
    (call $str_new (i32.add (global.get $rtdata) (local.get $offset)) (local.get $len)))
  (func $to_str (param $v i64) (result i64)
    (local $k i32) (local $i i32) (local $n i32) (local $out i64)
    (if (i64.eq (i64.and (local.get $v) (i64.const 1)) (i64.const 1))
      (then (return (call $int_to_str (local.get $v)))))
    (if (i64.eq (local.get $v) (i64.const 0x02)) (then (return (call $lit (i32.const 0) (i32.const 0)))))
    (if (i64.eq (local.get $v) (i64.const 0x12)) (then (return (call $lit (i32.const 0) (i32.const 4)))))
    (if (i64.eq (local.get $v) (i64.const 0x0a)) (then (return (call $lit (i32.const 4) (i32.const 5)))))
    (local.set $k (call $kind (local.get $v)))
    (if (i32.eq (local.get $k) (i32.const 1)) (then (return (local.get $v))))
    (if (i32.eq (local.get $k) (i32.const 2))
      (then
        (local.set $out (call $lit (i32.const 9) (i32.const 1)))
        (local.set $n (call $len (local.get $v)))
        (block $done
          (loop $next
            (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
            (if (local.get $i) (then (local.set $out (call $str_concat (local.get $out) (call $lit (i32.const 11) (i32.const 1))))))
            (local.set $out (call $str_concat (local.get $out) (call $to_str (call $list_get (local.get $v) (local.get $i)))))
            (local.set $i (i32.add (local.get $i) (i32.const 1)))
            (br $next)))
        (return (call $str_concat (local.get $out) (call $lit (i32.const 10) (i32.const 1))))))
    (call $lit (i32.const 12) (i32.const 7)))

  ;; Structural equality: every value can be compared, functions
  ;; included (by code and captured values).
  (func $eq (param $a i64) (param $b i64) (result i32)
    (local $k i32) (local $n i32) (local $i i32) (local $pa i32) (local $pb i32)
    (if (i64.eq (local.get $a) (local.get $b)) (then (return (i32.const 1))))
    (if (i32.eqz (i32.and (call $is_obj (local.get $a)) (call $is_obj (local.get $b)))) (then (return (i32.const 0))))
    (local.set $k (call $kind (local.get $a)))
    (if (i32.ne (local.get $k) (call $kind (local.get $b))) (then (return (i32.const 0))))
    (if (i32.eq (local.get $k) (i32.const 1)) (then (return (call $str_eq (local.get $a) (local.get $b)))))
    (if (i32.eq (local.get $k) (i32.const 4)) (then (return (call $dict_eq (local.get $a) (local.get $b)))))
    (local.set $pa (call $ptr (local.get $a)))
    (local.set $pb (call $ptr (local.get $b)))
    (local.set $n (call $len (local.get $a)))
    (if (i32.ne (local.get $n) (call $len (local.get $b))) (then (return (i32.const 0))))
    ;; Lists, views, and emits hold `n` values; records hold `n` pairs;
    ;; closures hold their code then `n` values.
    (if (i32.eq (local.get $k) (i32.const 3)) (then (local.set $n (i32.shl (local.get $n) (i32.const 1)))))
    (if (i32.eq (local.get $k) (i32.const 5))
      (then
        (if (i64.ne (i64.load offset=8 (local.get $pa)) (i64.load offset=8 (local.get $pb))) (then (return (i32.const 0))))
        (local.set $pa (i32.add (local.get $pa) (i32.const 8)))
        (local.set $pb (i32.add (local.get $pb) (i32.const 8)))))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (if (i32.eqz (call $eq
              (i64.load offset=8 (i32.add (local.get $pa) (i32.shl (local.get $i) (i32.const 3))))
              (i64.load offset=8 (i32.add (local.get $pb) (i32.shl (local.get $i) (i32.const 3))))))
          (then (return (i32.const 0))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (i32.const 1))

  ;; Lists
  (func $list_get (param $v i64) (param $i i32) (result i64)
    (i64.load offset=8 (i32.add (call $ptr (local.get $v)) (i32.shl (local.get $i) (i32.const 3)))))
  (func $list_set (param $p i32) (param $i i32) (param $x i64)
    (i64.store offset=8 (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 3))) (local.get $x)))
  (func $list_new (param $n i32) (result i32)
    (call $obj (i32.const 2) (local.get $n) (i32.shl (local.get $n) (i32.const 3))))
  (func $count (param $v i64) (result i64)
    (i64.or (i64.shl (i64.extend_i32_u (call $len (local.get $v))) (i64.const 1)) (i64.const 1)))
  (func $first (param $v i64) (result i64)
    (if (result i64) (call $len (local.get $v))
      (then (call $list_get (local.get $v) (i32.const 0)))
      (else (i64.const 0x02))))
  (func $slice (param $v i64) (param $from i32) (param $n i32) (result i64)
    (local $p i32)
    (local.set $p (call $list_new (local.get $n)))
    (memory.copy (i32.add (local.get $p) (i32.const 8))
      (i32.add (i32.add (call $ptr (local.get $v)) (i32.const 8)) (i32.shl (local.get $from) (i32.const 3)))
      (i32.shl (local.get $n) (i32.const 3)))
    (i64.extend_i32_u (local.get $p)))
  (func $rest (param $v i64) (result i64)
    (if (result i64) (call $len (local.get $v))
      (then (call $slice (local.get $v) (i32.const 1) (i32.sub (call $len (local.get $v)) (i32.const 1))))
      (else (local.get $v))))
  (func $concat (param $a i64) (param $b i64) (result i64)
    (local $p i32) (local $la i32) (local $lb i32)
    (local.set $la (call $len (local.get $a)))
    (local.set $lb (call $len (local.get $b)))
    (local.set $p (call $list_new (i32.add (local.get $la) (local.get $lb))))
    (memory.copy (i32.add (local.get $p) (i32.const 8)) (i32.add (call $ptr (local.get $a)) (i32.const 8)) (i32.shl (local.get $la) (i32.const 3)))
    (memory.copy (i32.add (i32.add (local.get $p) (i32.const 8)) (i32.shl (local.get $la) (i32.const 3)))
      (i32.add (call $ptr (local.get $b)) (i32.const 8)) (i32.shl (local.get $lb) (i32.const 3)))
    (i64.extend_i32_u (local.get $p)))
  (func $conj (param $a i64) (param $x i64) (result i64)
    (local $p i32) (local $n i32)
    (local.set $n (call $len (local.get $a)))
    (local.set $p (call $list_new (i32.add (local.get $n) (i32.const 1))))
    (memory.copy (i32.add (local.get $p) (i32.const 8)) (i32.add (call $ptr (local.get $a)) (i32.const 8)) (i32.shl (local.get $n) (i32.const 3)))
    (call $list_set (local.get $p) (local.get $n) (local.get $x))
    (i64.extend_i32_u (local.get $p)))
  (func $map (param $f i64) (param $xs i64) (result i64)
    (local $p i32) (local $n i32) (local $i i32)
    (local.set $n (call $len (local.get $xs)))
    (local.set $p (call $list_new (local.get $n)))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (call $list_set (local.get $p) (local.get $i)
          (call $apply1 (local.get $f) (call $list_get (local.get $xs) (local.get $i))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (i64.extend_i32_u (local.get $p)))
  (func $filter (param $f i64) (param $xs i64) (result i64)
    (local $p i32) (local $n i32) (local $i i32) (local $k i32) (local $x i64)
    (local.set $n (call $len (local.get $xs)))
    (local.set $p (call $list_new (local.get $n)))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (local.set $x (call $list_get (local.get $xs) (local.get $i)))
        (if (i64.eq (call $apply1 (local.get $f) (local.get $x)) (i64.const 0x12))
          (then
            (call $list_set (local.get $p) (local.get $k) (local.get $x))
            (local.set $k (i32.add (local.get $k) (i32.const 1)))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (i32.store offset=4 (local.get $p) (local.get $k))
    (i64.extend_i32_u (local.get $p)))

  ;; Records
  (func $rec_get (param $r i64) (param $id i64) (result i64)
    (local $p i32) (local $n i32) (local $i i32)
    (local.set $p (call $ptr (local.get $r)))
    (local.set $n (call $len (local.get $r)))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (if (i64.eq (i64.load offset=8 (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 4)))) (local.get $id))
          (then (return (i64.load offset=16 (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 4)))))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    ;; The type checker rules this out.
    unreachable)

  ;; Dictionaries: string keys, entries in insertion order.
  (func $dict_find (param $d i64) (param $key i64) (result i32)
    ;; The index of `key`, or -1.
    (local $p i32) (local $n i32) (local $i i32)
    (local.set $p (call $ptr (local.get $d)))
    (local.set $n (call $len (local.get $d)))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (if (call $str_eq (i64.load offset=8 (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 4)))) (local.get $key))
          (then (return (local.get $i))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (i32.const -1))
  (func $dict_key (param $d i64) (param $i i32) (result i64)
    (i64.load offset=8 (i32.add (call $ptr (local.get $d)) (i32.shl (local.get $i) (i32.const 4)))))
  (func $dict_val (param $d i64) (param $i i32) (result i64)
    (i64.load offset=16 (i32.add (call $ptr (local.get $d)) (i32.shl (local.get $i) (i32.const 4)))))
  (func $dict_get (param $d i64) (param $key i64) (result i64)
    (local $i i32)
    (local.set $i (call $dict_find (local.get $d) (local.get $key)))
    (if (result i64) (i32.lt_s (local.get $i) (i32.const 0))
      (then (i64.const 0x02))
      (else (call $dict_val (local.get $d) (local.get $i)))))
  (func $dict_new (param $n i32) (result i32)
    (call $obj (i32.const 4) (local.get $n) (i32.shl (local.get $n) (i32.const 4))))
  (func $dict_put (param $p i32) (param $i i32) (param $key i64) (param $val i64)
    (i64.store offset=8 (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 4))) (local.get $key))
    (i64.store offset=16 (i32.add (local.get $p) (i32.shl (local.get $i) (i32.const 4))) (local.get $val)))
  (func $dict_assoc (param $d i64) (param $key i64) (param $val i64) (result i64)
    (local $p i32) (local $n i32) (local $i i32)
    (local.set $n (call $len (local.get $d)))
    (local.set $i (call $dict_find (local.get $d) (local.get $key)))
    (if (i32.ge_s (local.get $i) (i32.const 0))
      (then
        (local.set $p (call $dict_new (local.get $n)))
        (memory.copy (i32.add (local.get $p) (i32.const 8)) (i32.add (call $ptr (local.get $d)) (i32.const 8)) (i32.shl (local.get $n) (i32.const 4)))
        (call $dict_put (local.get $p) (local.get $i) (local.get $key) (local.get $val)))
      (else
        (local.set $p (call $dict_new (i32.add (local.get $n) (i32.const 1))))
        (memory.copy (i32.add (local.get $p) (i32.const 8)) (i32.add (call $ptr (local.get $d)) (i32.const 8)) (i32.shl (local.get $n) (i32.const 4)))
        (call $dict_put (local.get $p) (local.get $n) (local.get $key) (local.get $val))))
    (i64.extend_i32_u (local.get $p)))
  (func $dict_dissoc (param $d i64) (param $key i64) (result i64)
    (local $p i32) (local $n i32) (local $i i32) (local $j i32)
    (local.set $n (call $len (local.get $d)))
    (if (i32.lt_s (call $dict_find (local.get $d) (local.get $key)) (i32.const 0)) (then (return (local.get $d))))
    (local.set $p (call $dict_new (i32.sub (local.get $n) (i32.const 1))))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (if (i32.eqz (call $str_eq (call $dict_key (local.get $d) (local.get $i)) (local.get $key)))
          (then
            (call $dict_put (local.get $p) (local.get $j) (call $dict_key (local.get $d) (local.get $i)) (call $dict_val (local.get $d) (local.get $i)))
            (local.set $j (i32.add (local.get $j) (i32.const 1)))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (i64.extend_i32_u (local.get $p)))
  (func $dict_eq (param $a i64) (param $b i64) (result i32)
    ;; Equal when they hold the same keys with equal values, in any order.
    (local $n i32) (local $i i32) (local $j i32)
    (local.set $n (call $len (local.get $a)))
    (if (i32.ne (local.get $n) (call $len (local.get $b))) (then (return (i32.const 0))))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (local.set $j (call $dict_find (local.get $b) (call $dict_key (local.get $a) (local.get $i))))
        (if (i32.lt_s (local.get $j) (i32.const 0)) (then (return (i32.const 0))))
        (if (i32.eqz (call $eq (call $dict_val (local.get $a) (local.get $i)) (call $dict_val (local.get $b) (local.get $j))))
          (then (return (i32.const 0))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (i32.const 1))

  ;; Closures
  (func $clo_new (param $index i32) (param $arity i32) (param $free i32) (result i32)
    (local $p i32)
    (local.set $p (call $obj (i32.const 5) (local.get $free) (i32.add (i32.const 8) (i32.shl (local.get $free) (i32.const 3)))))
    (i32.store offset=8 (local.get $p) (local.get $index))
    (i32.store offset=12 (local.get $p) (local.get $arity))
    (local.get $p))

  ;; Views and emits
  (func $view_new (param $tag i64) (param $attrs i64) (param $children i64) (param $events i64) (result i64)
    (local $p i32)
    (local.set $p (call $obj (i32.const 6) (i32.const 4) (i32.const 32)))
    (i64.store offset=8 (local.get $p) (local.get $tag))
    (i64.store offset=16 (local.get $p) (local.get $attrs))
    (i64.store offset=24 (local.get $p) (local.get $children))
    (i64.store offset=32 (local.get $p) (local.get $events))
    (i64.extend_i32_u (local.get $p)))
  (func $emit_new (param $name i64) (param $detail i64) (result i64)
    (local $p i32)
    (local.set $p (call $obj (i32.const 7) (i32.const 2) (i32.const 16)))
    (i64.store offset=8 (local.get $p) (local.get $name))
    (i64.store offset=16 (local.get $p) (local.get $detail))
    (i64.extend_i32_u (local.get $p)))
