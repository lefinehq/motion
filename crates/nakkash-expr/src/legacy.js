// The legacy engine's snake_case aliases.
$global("this_comp", function () { return thisComp; });
$global("this_layer", function () { return thisLayer; });
$global("this_property", function () { return thisProperty; });
$global("num_keys", function () { return numKeys; });
["in_point", "out_point", "start_time", "has_parent", "anchor_point", "time_remap"].forEach(function (n) {
  var camel = n.replace(/_([a-z])/g, function (m, c) { return c.toUpperCase(); });
  $global(n, function () { return $thisLayer()[camel]; });
});
var value_at_time = valueAtTime, velocity_at_time = velocityAtTime, nearest_key = nearestKey;
var loop_out = loopOut, loop_in = loopIn, loop_out_duration = loopOutDuration, loop_in_duration = loopInDuration;
var seed_random = seedRandom, gauss_random = gaussRandom, ease_in = easeIn, ease_out = easeOut;
var rgb_to_hsl = rgbToHsl, hsl_to_rgb = hslToRgb, degrees_to_radians = degreesToRadians, radians_to_degrees = radiansToDegrees;
var frames_to_time = framesToTime, time_to_frames = timeToFrames, posterize_time = posterizeTime;
