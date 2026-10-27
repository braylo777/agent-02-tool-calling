// Safe arithmetic parser (+ - * / % parentheses); no eval, which Workers forbid.
export function calc(expr) {
  const src = String(expr).replace(/\s+/g, "");
  const t = src.match(/\d+\.?\d*|[-+*/%()]/g) || [];
  if (!src || t.join("") !== src) throw new Error("invalid expression");
  let i = 0;
  const num = () => {
    if (t[i] === "-") { i++; return -num(); }
    if (t[i] === "(") { i++; const v = add(); if (t[i++] !== ")") throw new Error("invalid expression"); return v; }
    const v = Number(t[i++]); if (Number.isNaN(v)) throw new Error("invalid expression"); return v;
  };
  const mul = () => { let v = num(); while (t[i] && "*/%".includes(t[i])) { const o = t[i++], r = num(); v = o === "*" ? v * r : o === "/" ? v / r : v % r; } return v; };
  const add = () => { let v = mul(); while (t[i] === "+" || t[i] === "-") { const o = t[i++], r = mul(); v = o === "+" ? v + r : v - r; } return v; };
  const v = add(); if (i !== t.length) throw new Error("invalid expression"); return v;
}
