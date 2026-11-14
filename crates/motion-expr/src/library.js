// The expression language's functions that need no host. Arithmetic goes
// through $add, $sub, $mul, $div, $mod and $neg (the rewrite puts them in),
// with the language's array rules: arrays combine component by component,
// the shorter padded with zeros; a number with an array changes only the
// first component for + and -, and scales every component for * and /;
// a string on either side concatenates.

var $isArr = Array.isArray;
// A property read in arithmetic is its value: decorated numbers and strings
// become primitives, decorated arrays plain arrays.
function $v(x) {
  if (x !== null && typeof x === "object" && !$isArr(x)) return x.valueOf();
  return x;
}
function $add(a, b) {
  a = $v(a); b = $v(b);
  if (typeof a === "string" || typeof b === "string") return String(a) + String(b);
  var arrA = $isArr(a), arrB = $isArr(b);
  if (!arrA && !arrB) return a + b;
  if (arrA && arrB) {
    var n = Math.max(a.length, b.length), r = new Array(n);
    for (var i = 0; i < n; i++) r[i] = (i < a.length ? a[i] : 0) + (i < b.length ? b[i] : 0);
    return r;
  }
  var arr = arrA ? a : b, num = arrA ? b : a, out = arr.slice();
  out[0] = out[0] + num;
  return out;
}
function $neg(a) {
  a = $v(a);
  if ($isArr(a)) return a.map(function (x) { return -x; });
  return -a;
}
function $sub(a, b) {
  a = $v(a); b = $v(b);
  var arrA = $isArr(a), arrB = $isArr(b);
  if (!arrA && !arrB) return a - b;
  if (arrA && arrB) {
    var n = Math.max(a.length, b.length), r = new Array(n);
    for (var i = 0; i < n; i++) r[i] = (i < a.length ? a[i] : 0) - (i < b.length ? b[i] : 0);
    return r;
  }
  if (arrA) { var o = a.slice(); o[0] = o[0] - b; return o; }
  var p = $neg(b); p[0] = a + p[0]; return p;
}
function $mul(a, b) {
  a = $v(a); b = $v(b);
  var arrA = $isArr(a), arrB = $isArr(b);
  if (!arrA && !arrB) return a * b;
  if (arrA && arrB) {
    var n = Math.max(a.length, b.length), r = new Array(n);
    for (var i = 0; i < n; i++) r[i] = (i < a.length ? a[i] : 0) * (i < b.length ? b[i] : 0);
    return r;
  }
  var arr = arrA ? a : b, s = arrA ? b : a;
  return arr.map(function (x) { return x * s; });
}
function $div(a, b) {
  a = $v(a); b = $v(b);
  var arrA = $isArr(a), arrB = $isArr(b);
  if (!arrA && !arrB) return a / b;
  if (arrA && !arrB) return a.map(function (x) { return x / b; });
  if (arrA && arrB) {
    var n = Math.max(a.length, b.length), r = new Array(n);
    for (var i = 0; i < n; i++) r[i] = (i < a.length ? a[i] : 0) / (i < b.length ? b[i] : 0);
    return r;
  }
  return b.map(function (x) { return a / x; });
}
function $mod(a, b) {
  a = $v(a); b = $v(b);
  if ($isArr(a)) return a.map(function (x) { return x % b; });
  return a % b;
}

// Vector maths.
function add(a, b) { return $add(a, b); }
function sub(a, b) { return $sub(a, b); }
function mul(a, s) { return $mul(a, s); }
function div(a, s) { return $div(a, s); }
function clamp(v, lo, hi) {
  v = $v(v); lo = $v(lo); hi = $v(hi);
  if ($isArr(v)) return v.map(function (x, i) { return clamp(x, $isArr(lo) ? lo[i] : lo, $isArr(hi) ? hi[i] : hi); });
  return Math.min(Math.max(v, lo), hi);
}
function dot(a, b) { a = $v(a); b = $v(b); var s = 0; for (var i = 0; i < Math.min(a.length, b.length); i++) s += a[i] * b[i]; return s; }
function cross(a, b) { a = $v(a); b = $v(b); return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]; }
function length(a, b) {
  a = $v(a);
  if (b !== undefined) a = $sub(a, b);
  if (!$isArr(a)) return Math.abs(a);
  var s = 0; for (var i = 0; i < a.length; i++) s += a[i] * a[i];
  return Math.sqrt(s);
}
function normalize(a) { var l = length(a); return l === 0 ? $v(a) : $div(a, l); }


// Angles and time.
function degreesToRadians(d) { return d * Math.PI / 180; }
function radiansToDegrees(r) { return r * 180 / Math.PI; }
function framesToTime(frames, fps) { return frames / (fps === undefined ? 1 / thisComp.frameDuration : fps); }
function timeToFrames(t, fps) {
  if (t === undefined) t = time;
  return Math.floor(t * (fps === undefined ? 1 / thisComp.frameDuration : fps) + 1e-9);
}

// Interpolation. `linear` is a straight ramp; `ease`, `easeIn` and
// `easeOut` follow cubic curves whose ends are flat where the name says.
function $bezier(x1, y1, x2, y2) {
  return function (x) {
    if (x <= 0) return 0;
    if (x >= 1) return 1;
    var lo = 0, hi = 1, u = x;
    for (var i = 0; i < 40; i++) {
      var bx = 3 * (1 - u) * (1 - u) * u * x1 + 3 * (1 - u) * u * u * x2 + u * u * u;
      if (Math.abs(bx - x) < 1e-12) break;
      if (bx < x) lo = u; else hi = u;
      u = (lo + hi) / 2;
    }
    return 3 * (1 - u) * (1 - u) * u * y1 + 3 * (1 - u) * u * u * y2 + u * u * u;
  };
}
function $interpolate(shape, args) {
  var t = $v(args[0]), tMin, tMax, v1, v2;
  if (args.length >= 5) { tMin = $v(args[1]); tMax = $v(args[2]); v1 = $v(args[3]); v2 = $v(args[4]); }
  else { tMin = 0; tMax = 1; v1 = $v(args[1]); v2 = $v(args[2]); }
  var u = tMax === tMin ? (t >= tMax ? 1 : 0) : (t - tMin) / (tMax - tMin);
  u = shape(Math.min(Math.max(u, 0), 1));
  return $add(v1, $mul($sub(v2, v1), u));
}
var $easeBoth = $bezier(0.333, 0, 0.667, 1);
var $easeIn = $bezier(0.333, 0, 0.833, 0.833);
var $easeOut = $bezier(0.167, 0.167, 0.667, 1);
function linear() { return $interpolate(function (u) { return u; }, arguments); }
function ease() { return $interpolate($easeBoth, arguments); }
function easeIn() { return $interpolate($easeIn, arguments); }
function easeOut() { return $interpolate($easeOut, arguments); }

// Colour: RGBA from 0 to 1, as the language keeps it.
function rgbToHsl(c) {
  c = $v(c);
  var r = c[0], g = c[1], b = c[2], max = Math.max(r, g, b), min = Math.min(r, g, b);
  var h = 0, s = 0, l = (max + min) / 2, d = max - min;
  if (d !== 0) {
    s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
    if (max === r) h = (g - b) / d + (g < b ? 6 : 0);
    else if (max === g) h = (b - r) / d + 2;
    else h = (r - g) / d + 4;
    h /= 6;
  }
  return [h, s, l, c.length > 3 ? c[3] : 1];
}
function hslToRgb(c) {
  c = $v(c);
  var h = c[0], s = c[1], l = c[2];
  function hue(p, q, t) {
    if (t < 0) t += 1;
    if (t > 1) t -= 1;
    if (t < 1 / 6) return p + (q - p) * 6 * t;
    if (t < 1 / 2) return q;
    if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
    return p;
  }
  if (s === 0) return [l, l, l, c.length > 3 ? c[3] : 1];
  var q = l < 0.5 ? l * (1 + s) : l + s - l * s, p = 2 * l - q;
  return [hue(p, q, h + 1 / 3), hue(p, q, h), hue(p, q, h - 1 / 3), c.length > 3 ? c[3] : 1];
}
