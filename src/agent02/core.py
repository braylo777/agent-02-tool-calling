"""Shared helpers: JSON text, safe calculator, input cap."""
import json, math, re
from decimal import Decimal

MODEL = "@cf/meta/llama-3.3-70b-instruct-fp8-fast"
_TOKEN = re.compile(r"\d+\.?\d*|[-+*/%()]")


class CalcError(Exception):
    pass


def js_json(o) -> str:
    return json.dumps(o, separators=(",", ":"), ensure_ascii=False)


def text(r) -> str:
    v = r.get("response") if isinstance(r, dict) else None
    if isinstance(v, str):
        return v
    return js_json(v if v is not None else r)


def cap(s, n: int = 500) -> bool:
    return isinstance(s, str) and bool(s.strip()) and len(s) <= n


def fmt(v: float) -> str:
    """Format like JS String(+v.toPrecision(12))."""
    if math.isnan(v):
        return "NaN"
    if math.isinf(v):
        return "Infinity" if v > 0 else "-Infinity"
    v = float(f"{v:.12g}")
    if v == 0:
        return "0"
    if v == int(v) and abs(v) < 1e21:
        return str(int(v))
    if 1e-6 <= abs(v) < 1e21:
        return format(Decimal(repr(v)), "f")
    m, e = repr(v).split("e") if "e" in repr(v) else f"{v:e}".split("e")
    return f"{m}e{'+' if int(e) >= 0 else '-'}{abs(int(e))}"


def _div(a, b):
    if b == 0:
        if a == 0 or math.isnan(a):
            return math.nan
        return -math.inf if (a < 0) != (math.copysign(1.0, b) < 0) else math.inf
    return a / b


def _mod(a, b):
    if b == 0 or math.isinf(a) or math.isnan(a) or math.isnan(b):
        return math.nan
    return math.fmod(a, b)


def calc(expr) -> float:
    src = re.sub(r"\s+", "", str(expr))
    t = _TOKEN.findall(src)
    if not src or "".join(t) != src:
        raise CalcError("invalid expression")
    i = 0

    def peek():
        return t[i] if i < len(t) else None

    def num():
        nonlocal i
        if peek() == "-":
            i += 1
            return -num()
        if peek() == "(":
            i += 1
            v = add()
            if peek() != ")":
                raise CalcError("invalid expression")
            i += 1
            return v
        tok = peek()
        i += 1
        try:
            return float(tok)
        except (TypeError, ValueError):
            raise CalcError("invalid expression")

    def mul():
        nonlocal i
        v = num()
        while peek() in ("*", "/", "%"):
            o = t[i]; i += 1
            r = num()
            v = v * r if o == "*" else _div(v, r) if o == "/" else _mod(v, r)
        return v

    def add():
        nonlocal i
        v = mul()
        while peek() in ("+", "-"):
            o = t[i]; i += 1
            r = mul()
            v = v + r if o == "+" else v - r
        return v

    v = add()
    if i != len(t):
        raise CalcError("invalid expression")
    return v
