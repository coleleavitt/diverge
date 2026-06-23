#!/usr/bin/env python3
"""Differential-test oracle: emit real upstream Portage outputs as TSV lines.

This script imports the actual Gentoo Portage functions from the read-only
reference checkout (``research/portage/lib``) and evaluates them over input
vectors transcribed from the upstream test files under
``research/portage/lib/portage/tests``. Each result is printed as one
tab-separated record so the Rust differential harness
(``tests/interop_differential.rs``) can cross-check diverge's Rust ports
against genuine emerge behavior instead of hand-written expectations.

Record schema (fields separated by TAB):

    vercmp              <left> <right> <sign>
    cpv_sort            <space-joined input> <space-joined output>
    get_operator        <atom> <op|NUL>
    dep_getcpv          <atom> <cpv|NUL>
    dep_getslot         <atom> <slot|NUL>
    dep_getrepo         <atom> <repo|NUL>
    dep_getusedeps      <atom> <space-joined flags | ERR>
    isjustname          <atom> <true|false>
    paren_reduce        <dep> <canonical | ERR>
    use_reduce          <dep> <uselist> <masklist> <excludeall> <subset|NUL> <matchall> <canonical | ERR>
    check_required_use  <required_use> <use> <iuse> <eapi|NUL> <true|false|ERR>

``NUL`` is the literal escape ``\\x00`` and ``ERR`` is ``\\x01``; the Rust
harness maps ``None``/``Err`` to the same sentinels. Nested dependency
structures are serialized with :func:`canon`, the same paren-enclosed form
diverge's ``paren_enclose`` produces.

Usage:
    PYTHONPATH=<repo>/research/portage/lib python3 portage_oracle.py

Exit codes:
    0  records emitted
    77 portage could not be imported (caller should treat as "skip")
"""

import sys

NUL = "\x00"
ERR = "\x01"


def canon(value):
    """Canonical paren-enclosed serialization matching Rust ``paren_enclose``."""
    if isinstance(value, list):
        return "( " + " ".join(canon(item) for item in value) + " )"
    return str(value)


def emit(out, *fields):
    out.write("\t".join(fields) + "\n")


def main() -> int:
    try:
        from portage.dep import (
            check_required_use,
            dep_getcpv,
            dep_getrepo,
            dep_getslot,
            dep_getusedeps,
            get_operator,
            isjustname,
            paren_reduce,
            use_reduce,
        )
        from portage.exception import InvalidAtom, InvalidDependString
        from portage.util import varexpand
        from portage.versions import cpv_sort_key, vercmp
    except Exception as exc:  # pragma: no cover - only without portage
        sys.stderr.write(f"portage import failed: {exc}\n")
        return 77

    out = sys.stdout

    # vercmp - tests/versions/test_vercmp.py
    vercmp_pairs = [
        ("6.0", "5.0"), ("5.0", "5"), ("1.0-r1", "1.0-r0"), ("1.0-r1", "1.0"),
        ("999999999999999999999999999999", "999999999999999999999999999998"),
        ("1.0.0", "1.0"), ("1.0.0", "1.0b"), ("1b", "1"), ("1b_p1", "1_p1"),
        ("1.1b", "1.1"), ("12.2.5", "12.2b"), ("4.0", "5.0"), ("5", "5.0"),
        ("1.0_pre2", "1.0_p2"), ("1.0_alpha2", "1.0_p2"),
        ("1.0_alpha1", "1.0_beta1"), ("1.0_beta3", "1.0_rc3"),
        ("1.001000000000000000001", "1.001000000000000000002"),
        ("1.00100000000", "1.0010000000000000001"),
        ("999999999999999999999999999998", "999999999999999999999999999999"),
        ("1.01", "1.1"), ("1.0-r0", "1.0-r1"), ("1.0", "1.0-r1"),
        ("1.0", "1.0.0"), ("1.0b", "1.0.0"), ("1_p1", "1b_p1"), ("1", "1b"),
        ("1.1", "1.1b"), ("12.2b", "12.2.5"), ("4.0", "4.0"), ("1.0", "1.0"),
        ("1.0-r0", "1.0"), ("1.0", "1.0-r0"), ("1.0-r0", "1.0-r0"),
        ("1.0-r1", "1.0-r1"), ("1", "2"), ("1.0_alpha", "1.0_pre"),
        ("1.0_beta", "1.0_alpha"), ("0", "0.0"), ("1_p1", "1b_p1"),
        ("1.1b", "1.1"), ("12.2b", "12.2"),
    ]
    for left, right in vercmp_pairs:
        result = vercmp(left, right)
        if result is None:
            sign = ERR
        else:
            sign = "0" if result == 0 else ("1" if result > 0 else "-1")
        emit(out, "vercmp", left, right, sign)

    # cpv_sort_key - tests/versions/test_cpv_sort_key.py
    cpv_sort_inputs = [
        ["a/b-2_alpha", "a", "b", "a/b-2", "a/a-1", "a/b-1"],
        ["sys-apps/portage-2.1", "sys-apps/portage-2.1-r1", "sys-apps/portage-2.0"],
        ["x/y-1.0", "x/y-1.0_alpha", "x/y-1.0_p1", "x/y-1.0-r1"],
    ]
    for values in cpv_sort_inputs:
        ordered = sorted(values, key=cpv_sort_key())
        emit(out, "cpv_sort", " ".join(values), " ".join(ordered))

    # dep accessors - tests/dep/test_dep_get*.py, test_get_operator.py,
    # test_isjustname.py
    accessor_inputs = [
        "sys-apps/portage", "=sys-apps/portage-2.1", ">=sys-apps/portage-2.1",
        "<=sys-apps/portage-2.1", ">sys-apps/portage-2.1", "<sys-apps/portage-2.1",
        "~sys-apps/portage-2.1", "=sys-apps/portage-2.1*", "=sys-apps/portage-2.1:0",
        "=sys-apps/portage-2.1:foo", "sys-apps/portage:3",
        "app-doc/php-docs-20071125", "virtual/ffmpeg:0/53", "virtual/ffmpeg:0/53=",
        "virtual/ffmpeg:=", "virtual/ffmpeg:*", "=dev-libs/A-1[foo,-bar]",
        "dev-libs/A:2[a,b]", "media-libs/x264-20060810", "games-strategy/ufo2000",
        "foo/bar-1", "portage", "=portage-2.1", "sys-apps/portage:0/1.2",
    ]
    for mydep in accessor_inputs:
        emit(out, "get_operator", mydep, _opt(_safe(get_operator, mydep)))
        emit(out, "dep_getcpv", mydep, _opt(_safe(dep_getcpv, mydep)))
        emit(out, "dep_getslot", mydep, _opt(_safe(dep_getslot, mydep)))
        emit(out, "dep_getrepo", mydep, _opt(_safe(dep_getrepo, mydep)))
        emit(out, "isjustname", mydep, _b(_safe(isjustname, mydep)))

    repo_inputs = [
        "sys-apps/portage::gentoo", "=sys-apps/portage-2.1::gentoo",
        "sys-apps/portage::gentoo[use]", "=sys-apps/portage-2.1::repo-name[use]",
        "app-misc/test::repository", "app-misc/test::repo123[a,b]",
    ]
    for mydep in repo_inputs:
        emit(out, "dep_getrepo", mydep, _opt(_safe(dep_getrepo, mydep)))
        emit(out, "dep_getslot", mydep, _opt(_safe(dep_getslot, mydep)))

    usedeps_inputs = [
        "=dev-libs/A-1[foo]", "=dev-libs/A-1[-bar]", "=dev-libs/A-1[foo,bar]",
        "=dev-libs/A-1[foo,-bar]", "=dev-libs/A-1[foo?,!bar?]",
        "=dev-libs/A-1:2[foo,bar]", "dev-libs/A[foo(+)]",
        "dev-libs/A[a(+),b(-)=,!c(+)=,d(-)?,!e(+)?,-f(-)]",
    ]
    for mydep in usedeps_inputs:
        try:
            value = " ".join(dep_getusedeps(mydep))
        except (InvalidAtom, InvalidDependString):
            value = ERR
        emit(out, "dep_getusedeps", mydep, value)

    # paren_reduce - tests/dep/test_paren_reduce.py
    paren_inputs = [
        "A", "( A )", "|| ( A B )", "|| ( A || ( B C ) )",
        "|| ( A || ( B C D ) )", "|| ( A || ( B || ( C D ) E ) )", "a? ( A )",
        "( || ( ( ( A ) B ) ) )", "( || ( || ( ( A ) B ) ) )", "|| ( A )",
        "( || ( || ( || ( A ) foo? ( B ) ) ) )",
        "( || ( || ( bar? ( A ) || ( foo? ( B ) ) ) ) )",
        "A || ( ) foo? ( ) B", "|| ( A ) || ( B )", "foo? ( A ) foo? ( B )",
        "|| ( ( A B ) C )", "|| ( ( A B ) ( C ) )",
        ">=dev-lang/php-5.2[pcre(+)]",
        # xfail (InvalidDependString) cases:
        "( A", "A )", "||( A B )", "|| (A B )", "|| ( A B)", "|| ( A B",
        "|| A B )", "|| A B", "|| ( A B ) )", "|| || B C", "|| ( A B || )",
        "a? A",
    ]
    for dep_str in paren_inputs:
        try:
            value = canon(paren_reduce(dep_str, _deprecation_warn=False))
        except InvalidDependString:
            value = ERR
        emit(out, "paren_reduce", dep_str, value)

    # use_reduce - tests/dep/test_use_reduce.py (implemented-feature subset)
    base = "a? ( A ) b? ( B ) !c? ( C ) !d? ( D )"
    use_reduce_cases = [
        (base, {"uselist": ["a", "b", "c", "d"]}),
        (base, {"uselist": ["a", "b", "c"]}),
        (base, {"uselist": ["b", "c"]}),
        (base, {"matchall": True}),
        (base, {"masklist": ["a", "c"]}),
        (base, {"matchall": True, "masklist": ["a", "c"]}),
        (base, {"uselist": ["a", "b"], "masklist": ["a", "c"]}),
        (base, {"excludeall": ["a", "c"]}),
        (base, {"uselist": ["b"], "excludeall": ["a", "c"]}),
        (base, {"matchall": True, "excludeall": ["a", "c"]}),
        (base, {"uselist": ["a", "b", "c", "d"], "subset": ["b"]}),
        ("|| ( foo bar? ( baz ) )", {"uselist": ["bar"], "subset": ["bar"]}),
        ("|| ( A B )", {}),
        ("|| ( A || ( B C ) )", {}),
        ("|| ( ( A B ) C )", {}),
        ("a? ( A ) || ( B C )", {"uselist": ["a"]}),
        # xfail cases:
        ("? ( A )", {}), ("!? ( A )", {}), ("( A", {}), ("A )", {}),
        ("||( A B )", {}), ("|| ( A B", {}), ("a? A", {}), ("foo?", {}),
        ("|| ( )", {}), ("foo? ( )", {}), ("1.0? ( A )", {}),
    ]
    for dep_str, kwargs in use_reduce_cases:
        try:
            value = canon(use_reduce(dep_str, **kwargs))
        except (InvalidDependString, ValueError):
            value = ERR
        emit(
            out,
            "use_reduce",
            dep_str,
            " ".join(kwargs.get("uselist", [])),
            " ".join(kwargs.get("masklist", [])),
            " ".join(kwargs.get("excludeall", [])),
            " ".join(kwargs["subset"]) if "subset" in kwargs else NUL,
            "1" if kwargs.get("matchall") else "0",
            value,
        )

    # check_required_use - tests/dep/test_check_required_use.py
    cru_iuse = ["a", "b", "c", "d"]
    cru_cases = [
        ("|| ( a b )", []), ("|| ( a b )", ["a"]), ("|| ( a b )", ["b"]),
        ("|| ( a b )", ["a", "b"]), ("^^ ( a b )", []), ("^^ ( a b )", ["a"]),
        ("^^ ( a b )", ["a", "b"]), ("?? ( a b )", ["a", "b"]),
        ("?? ( a b )", ["a"]), ("?? ( a b )", []), ("?? ( )", []),
        ("^^ ( || ( a b ) c )", []), ("^^ ( || ( a b ) c )", ["a"]),
        ("^^ ( || ( ( a b ) ) ( c ) )", []),
        ("( ^^ ( ( || ( ( a ) ( b ) ) ) ( ( c ) ) ) )", ["a"]),
        ("a || ( b c )", ["a"]), ("|| ( b c ) a", ["a"]),
        ("|| ( a b c )", ["a"]), ("^^ ( a b c )", ["a", "b"]),
        ("a? ( ^^ ( b c ) )", []), ("a? ( ^^ ( b c ) )", ["a"]),
        ("a? ( ^^ ( b c ) )", ["a", "b"]),
        ("^^ ( a? ( !b ) !c? ( d ) )", []),
        ("^^ ( a? ( !b ) !c? ( d ) )", ["a"]),
        ("^^ ( a? ( !b ) !c? ( d ) )", ["c"]),
        ("^^ ( a? ( !b ) !c? ( d ) )", ["a", "c"]),
        ("|| ( ^^ ( a b ) ^^ ( b c ) )", ["a", "b", "c"]),
        ("^^ ( || ( a b ) ^^ ( b c ) )", ["b"]),
        ("|| ( ( a b ) c )", ["a"]), ("|| ( ( a b ) c )", ["a", "b"]),
        ("^^ ( ( a b ) c )", ["a", "b", "c"]), ("^^ ( ( a b ) c )", ["c"]),
        # xfail (InvalidDependString) cases:
        ("^^ ( || ( a b ) ^^ ( b c )", ["a", "b", "c"]),
        ("^^( || ( a b ) ^^ ( b c ) )", ["a", "b", "c"]),
        ("^^ || ( a b ) ^^ ( b c )", ["a", "b", "c"]),
        ("^^ ( || ( a b ) ) ^^ ( b c ) )", ["a", "b", "c"]),
    ]
    for required_use, use in cru_cases:
        try:
            value = _b(bool(check_required_use(required_use, use, cru_iuse.__contains__)))
        except InvalidDependString:
            value = ERR
        emit(
            out,
            "check_required_use",
            required_use,
            " ".join(use),
            " ".join(cru_iuse),
            NUL,
            value,
        )

    # varexpand - tests/util/test_varExpand.py
    # Inputs/outputs contain backslashes, quotes and newlines, so the input,
    # serialized dict, and output are base64-encoded to stay TSV-safe.
    ve_dict_abc = {"a": "5", "b": "7", "c": "-5"}
    ve_dict_a = {"a": "5"}
    ve_cases = [
        ("$a", ve_dict_abc), ("${a}", ve_dict_abc),
        ("$b", ve_dict_abc), ("${b}", ve_dict_abc),
        ("$c", ve_dict_abc), ("${c}", ve_dict_abc),
        ("\\", {}), ("\\\\", {}), ("\\\\\\", {}), ("\\\\\\\\", {}),
        ("\\$", {}), ("\\\\$", {}), ("\\a", {}), ("\\b", {}),
        ("\\n", {}), ("\\r", {}), ("\\t", {}), ("\\\n", {}),
        ("\\\"", {}), ("\\'", {}),
        ("\"${a}\"", ve_dict_a), ("'${a}'", ve_dict_a),
        ("$fail", ve_dict_abc), ("${fail}", ve_dict_abc),
        ("${unclosed", ve_dict_a), ("${}", ve_dict_a),
        ("pre${a}post", ve_dict_a), ("$a$b$c", ve_dict_abc),
        ("trailing$", ve_dict_a), ("a\nb", ve_dict_a),
    ]
    for mystring, mydict in ve_cases:
        result = varexpand(mystring, mydict)
        emit(
            out,
            "varexpand",
            _b64(mystring),
            _b64_dict(mydict),
            _b64(result),
        )

    return 0


def _b64(text):
    import base64
    return base64.b64encode(text.encode("utf-8")).decode("ascii")


def _b64_dict(mydict):
    # Serialize as k1=v1\x1fk2=v2 then base64, so order-independent compare.
    items = "\x1f".join(f"{k}={v}" for k, v in sorted(mydict.items()))
    return _b64(items)


def _safe(func, *args):
    try:
        return func(*args)
    except Exception:  # noqa: BLE001 - any portage error => None marker
        return None


def _opt(value):
    return NUL if value is None else str(value)


def _b(value):
    return "true" if value else "false"


if __name__ == "__main__":
    sys.exit(main())
