// The expression objects over the host: comps, layers, property groups and
// properties, read through a few natives:
//   __kind(id)          0 project, 1 comp, 2 layer, 3 group, 4 property
//   __child(id, s, a)   the child by index (number) or name; by attribute
//                       name (`position`, `transform`) when `a`; -1 if none
//   __up(id)            what holds it, -1 at the top
//   __link(id, name)    a link such as a layer's `parent`, -1 if none
//   __attr(id, name)    an attribute (`index`, `width`, `frameDuration`)
//   __valueAt(id, t, own)  the value at t; `own` is the property being baked,
//                       whose value is the one under its expression
//   __numKeys(id), __keyTime(id, n), __keyValue(id, n)
//   __rectAt(id, t)     a layer's source rectangle [left, top, width, height]
//   __this()            the property being baked
// `time` is set before each run.

var $H = 0.0005; // half the step of a numeric velocity, in seconds

function $node(id) {
  if (id < 0) return undefined;
  switch (__kind(id)) {
    case 1: return $comp(id);
    case 2: return $layer(id);
    case 3: return $group(id);
    default: return $prop(id);
  }
}
function $childOrThrow(id, sel, what) {
  var c = __child(id, $v(sel), false);
  if (c < 0) throw new Error(what + " " + JSON.stringify($v(sel)) + " not found");
  return c;
}
// A callable object whose unknown attributes resolve through the host.
// A function's own `name` and `length` must not hide the node's, so only
// the methods given and `$id` come from the object itself.
function $callable(id, call, methods) {
  var f = function (sel) { return call(sel); };
  return new Proxy(f, {
    get: function (t, k) {
      if (k === "$id") return id;
      if (typeof k !== "string") return t[k];
      if (Object.prototype.hasOwnProperty.call(methods, k)) return methods[k];
      var a = __attr(id, k);
      if (a !== undefined) return a;
      var c = __child(id, k, true);
      if (c >= 0) return $node(c);
      return k === "apply" || k === "call" || k === "bind" ? t[k] : undefined;
    }
  });
}
function $group(id) {
  return $callable(id, function (sel) { return $node($childOrThrow(id, sel, "property")); }, {
    propertyGroup: function (n) { return $up(id, n); }
  });
}
function $up(id, n) {
  var u = id;
  for (var i = 0; i < (n === undefined ? 1 : n); i++) u = __up(u);
  return $node(u);
}
function $layer(id) {
  return $callable(id, function (sel) { return $node($childOrThrow(id, sel, "property")); }, {
    effect: function (sel) { return $node($childOrThrow(__child(id, "effect", true), sel, "effect")); },
    mask: function (sel) { return $node($childOrThrow(__child(id, "mask", true), sel, "mask")); },
    content: function (sel) { return $node($childOrThrow(__child(id, "content", true), sel, "content")); },
    sourceRectAtTime: function (t) {
      var r = __rectAt(id, t === undefined ? time : $v(t));
      return { left: r[0], top: r[1], width: r[2], height: r[3] };
    },
    get parent() { return $node(__link(id, "parent")); },
    get hasParent() { return __link(id, "parent") >= 0; },
    get index() { return __attr(id, "index"); }
  });
}
function $comp(id) {
  return $callable(id, function (sel) { return $comp(id).layer(sel); }, {
    layer: function (sel, rel) {
      if (rel !== undefined) sel = $v(sel).index + $v(rel);
      else if (sel !== null && typeof sel === "function" && sel.$id !== undefined) return sel;
      return $layer($childOrThrow(id, sel, "layer"));
    }
  });
}

// Properties are their value, with the property's methods on it.
var $PropMethods = {
  valueAtTime: function (t) { return __valueAt(this.$id, $v(t), this.$own); },
  velocityAtTime: function (t) { return $velocity(this.$id, $v(t), this.$own); },
  key: function (n) { return $key(this.$id, $v(n)); },
  nearestKey: function (t) { return $nearestKey(this.$id, $v(t)); },
  loopOut: function (type, n) { return $loop(this.$id, true, type, n, undefined); },
  loopIn: function (type, n) { return $loop(this.$id, false, type, n, undefined); },
  loopOutDuration: function (type, d) { return $loop(this.$id, true, type, 0, d); },
  loopInDuration: function (type, d) { return $loop(this.$id, false, type, 0, d); },
  propertyGroup: function (n) { return $up(this.$id, n); },
  smooth: function (width, samples, t) { return $smooth(this.$id, this.$own, width, samples, t); },
  wiggle: function (f, a, o, m, t) { return $wiggle(this.$id, this.$own, f, a, o, m, t); }
};
var $PropGetters = {
  value: function () { return __valueAt(this.$id, time, this.$own); },
  velocity: function () { return $velocity(this.$id, time, this.$own); },
  speed: function () { return length($velocity(this.$id, time, this.$own)); },
  numKeys: function () { return __numKeys(this.$id); },
  propertyIndex: function () { return __attr(this.$id, "propertyIndex"); },
  name: function () { return __attr(this.$id, "name"); }
};
function $decorate(proto) {
  for (var k in $PropMethods) proto[k] = $PropMethods[k];
  for (var g in $PropGetters) Object.defineProperty(proto, g, { get: $PropGetters[g] });
  return proto;
}
var $ArrayProp = $decorate(Object.create(Array.prototype));
var $NumberProp = $decorate({
  valueOf: function () { return this.$value; },
  toString: function () { return String(this.$value); },
  toFixed: function (d) { return this.$value.toFixed(d); }
});
var $TextProp = $decorate({
  valueOf: function () { return this.$value; },
  toString: function () { return this.$value; }
});
function $prop(id) {
  var own = id === __this();
  var v = __valueAt(id, time, own), o;
  if ($isArr(v)) { o = v.slice(); Object.setPrototypeOf(o, $ArrayProp); }
  else { o = Object.create(typeof v === "string" ? $TextProp : $NumberProp); o.$value = v; }
  Object.defineProperty(o, "$id", { value: id });
  Object.defineProperty(o, "$own", { value: own });
  return o;
}
function $velocity(id, t, own) {
  return $div($sub(__valueAt(id, t + $H, own), __valueAt(id, t - $H, own)), 2 * $H);
}

// Keys: their time, index and value, and the value's components by index;
// a component the value lacks throws, as scripts that probe it expect.
function $key(id, n) {
  if (!(n >= 1 && n <= __numKeys(id))) throw new Error("key " + n + " out of range");
  var v = __keyValue(id, n), k = { time: __keyTime(id, n), index: n, value: v };
  var dims = $isArr(v) ? v.length : 1;
  for (var i = 0; i < 4; i++) {
    if (i < dims) k[i] = $isArr(v) ? v[i] : v;
    else Object.defineProperty(k, i, { get: function () { throw new Error("index out of range"); } });
  }
  return k;
}
function $nearestKey(id, t) {
  var n = __numKeys(id), best = 1;
  if (n < 1) throw new Error("the property has no keys");
  for (var i = 2; i <= n; i++) if (Math.abs(__keyTime(id, i) - t) < Math.abs(__keyTime(id, best) - t)) best = i;
  return $key(id, best);
}

// Loops past the last key (out) or before the first (in).
function $loop(id, out, type, count, duration) {
  type = type === undefined ? "cycle" : String($v(type));
  count = count === undefined ? 0 : $v(count);
  var n = __numKeys(id), own = id === __this();
  if (n < 2) return __valueAt(id, time, own);
  var first = __keyTime(id, 1), last = __keyTime(id, n), start, end;
  if (out) {
    if (time <= last) return __valueAt(id, time, own);
    end = last;
    start = duration !== undefined && $v(duration) > 0 ? last - $v(duration)
      : count > 0 && count < n ? __keyTime(id, n - count) : first;
  } else {
    if (time >= first) return __valueAt(id, time, own);
    start = first;
    end = duration !== undefined && $v(duration) > 0 ? first + $v(duration)
      : count > 0 && count < n ? __keyTime(id, 1 + count) : last;
  }
  var span = end - start;
  if (!(span > 0)) return __valueAt(id, time, own);
  var at = function (t) { return __valueAt(id, t, own); };
  if (type === "continue") {
    var edge = out ? end : start, v = $velocity(id, out ? edge - $H : edge + $H, own);
    return $add(at(edge), $mul(v, time - edge));
  }
  var d = out ? time - start : end - time, cycles = Math.floor(d / span), frac = d - cycles * span;
  if (type === "pingpong") {
    var forward = cycles % 2 === 0;
    if (out) return at(forward ? start + frac : end - frac);
    return at(forward ? end - frac : start + frac);
  }
  if (type === "offset") {
    var delta = $sub(at(end), at(start));
    if (out) return $add(at(start + frac), $mul(delta, cycles));
    return $sub(at(end - frac), $mul(delta, cycles + 1));
  }
  return at(out ? start + frac : end - frac);
}

function $smooth(id, own, width, samples, t) {
  width = width === undefined ? 0.2 : $v(width);
  samples = samples === undefined ? 5 : $v(samples);
  t = t === undefined ? time : $v(t);
  if (samples < 2) return __valueAt(id, t, own);
  var sum = 0;
  for (var i = 0; i < samples; i++) sum = $add(sum, __valueAt(id, t - width / 2 + width * i / (samples - 1), own));
  return $div(sum, samples);
}

function $wiggle(id, own, freq, amp, octaves, mult, t) {
  freq = $v(freq); amp = $v(amp);
  octaves = octaves === undefined ? 1 : $v(octaves);
  mult = mult === undefined ? 0.5 : $v(mult);
  t = t === undefined ? time : $v(t);
  var v = __valueAt(id, t, own), dims = $isArr(v) ? v.length : 1, out = $isArr(v) ? v.slice() : v;
  for (var d = 0; d < dims; d++) {
    var sum = 0, a = 1, f = freq;
    for (var o = 0; o < Math.max(1, Math.floor(octaves)); o++) { sum += a * __noise(id, d, o, t * f); a *= mult; f *= 2; }
    if ($isArr(out)) out[d] += amp * sum; else out += amp * sum;
  }
  return out;
}

// What an expression may name without a layer in front: the property it
// is on, and the layer and comp holding it.
var $over = {};
function $global(name, get) {
  Object.defineProperty(globalThis, name, {
    get: function () { return name in $over ? $over[name] : get(); },
    set: function (v) { $over[name] = v; },
    configurable: false
  });
}
function $thisLayer() { var u = __this(); while (u >= 0 && __kind(u) !== 2) u = __up(u); return $layer(u); }
function $thisComp() { var u = __this(); while (u >= 0 && __kind(u) !== 1) u = __up(u); return $comp(u); }
$global("thisProperty", function () { return $prop(__this()); });
$global("thisLayer", function () { return $thisLayer(); });
$global("thisComp", function () { return $thisComp(); });
$global("value", function () { return __valueAt(__this(), time, true); });
$global("velocity", function () { return $velocity(__this(), time, true); });
$global("numKeys", function () { return __numKeys(__this()); });
["index", "inPoint", "outPoint", "startTime", "width", "height", "name", "hasParent", "parent",
 "transform", "anchorPoint", "position", "scale", "rotation", "opacity", "marker", "text",
 "timeRemap"].forEach(function (n) { $global(n, function () { return $thisLayer()[n]; }); });
function comp(name) { return $comp($childOrThrow(__project(), name, "comp")); }
function key(n) { return $key(__this(), $v(n)); }
function nearestKey(t) { return $nearestKey(__this(), $v(t)); }
function valueAtTime(t) { return __valueAt(__this(), $v(t), true); }
function velocityAtTime(t) { return $velocity(__this(), $v(t), true); }
function loopOut(type, n) { return $loop(__this(), true, type, n, undefined); }
function loopIn(type, n) { return $loop(__this(), false, type, n, undefined); }
function loopOutDuration(type, d) { return $loop(__this(), true, type, 0, d); }
function loopInDuration(type, d) { return $loop(__this(), false, type, 0, d); }
function smooth(w, s, t) { return $smooth(__this(), true, w, s, t); }
function wiggle(f, a, o, m, t) { return $wiggle(__this(), true, f, a, o, m, t); }
function effect(sel) { return $thisLayer().effect(sel); }
function mask(sel) { return $thisLayer().mask(sel); }
function content(sel) { return $thisLayer().content(sel); }
function sourceRectAtTime(t) { return $thisLayer().sourceRectAtTime(t); }
function posterizeTime(fps) { fps = $v(fps); if (fps > 0) time = Math.floor(time * fps + 1e-9) / fps; return fps; }
function seedRandom(offset, timeless) { __seedRandom($v(offset), !!timeless); }
function random(a, b) {
  a = $v(a); b = $v(b);
  if (a === undefined) return __random();
  if (b === undefined) return $isArr(a) ? a.map(function (x) { return x * __random(); }) : a * __random();
  if ($isArr(a)) return a.map(function (x, i) { return x + (b[i] - x) * __random(); });
  return a + (b - a) * __random();
}
function gaussRandom(a, b) {
  var g = function () { var u = 0; for (var i = 0; i < 12; i++) u += __random(); return (u - 6) / 6 * 0.5 + 0.5; };
  a = $v(a); b = $v(b);
  if (a === undefined) return g();
  if (b === undefined) return $isArr(a) ? a.map(function (x) { return x * g(); }) : a * g();
  if ($isArr(a)) return a.map(function (x, i) { return x + (b[i] - x) * g(); });
  return a + (b - a) * g();
}
function noise(p) { p = $v(p); return __noise(-1, 0, 0, $isArr(p) ? p[0] + 31.7 * (p[1] || 0) + 57.3 * (p[2] || 0) : p); }

// Runs leave nothing behind: every global a run made goes, and the
// overrides of the names above are cleared.
var $baseline = {};
function $mark() { Object.getOwnPropertyNames(globalThis).forEach(function (k) { $baseline[k] = true; }); }
function $reset() {
  $over = {};
  var names = Object.getOwnPropertyNames(globalThis);
  for (var i = 0; i < names.length; i++) {
    var k = names[i];
    if ($baseline[k] === true) continue;
    if (!delete globalThis[k]) globalThis[k] = undefined;
  }
}
