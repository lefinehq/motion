// An out-expo ease over every pair of keys, written the way the corpus's
// easing scripts are: find the keys around `time`, probe how many
// dimensions the value has by reading components until one throws, and
// ease each dimension.

function outExpo(t, start, change, duration) {
  var CORRECTION = 1.000976;
  return (t == duration) ? start + change : change * CORRECTION * (-Math.pow(2, -10 * t / duration) + 1) + start;
}

function easeKeys() {
  var n = 0;
  if (numKeys > 0) {
    n = nearestKey(time).index;
    if (key(n).time > time) { n-- }
  }

  try {
    var key1 = key(n);
    var key2 = key(n + 1);
  } catch (e) {
    return null;
  }

  var dims = 1;
  try {
    key(1)[1];
    dims = 2;
    key(1)[2];
    dims = 3;
  } catch (e) {}

  t = time - key1.time;
  d = key2.time - key1.time;

  if ((time < key1.time) || (time > key2.time)) {
    return value;
  }
  var out = [];
  for (var i = 0; i < dims; i++) {
    out.push(outExpo(t, key1[i], key2[i] - key1[i], d));
  }
  switch (dims) {
    case 1:
      return out[0];
    default:
      return out;
  }
}

(easeKeys() || value);
