# Entry point called by FlightGear's add-on framework (see
# $FG_ROOT/Docs/README.add-ons). Listeners registered here are tracked by the
# framework and removed automatically when the add-on is unloaded or reloaded.
var main = func(addon) {
    var autostart_trigger = "/sim/remote/c172/autostart";
    setlistener(autostart_trigger, func(n) {
        if (n.getValue() == 1) {
            c172p.autostart();
            setprop("/controls/gear/brake-parking", 1);
            setprop(autostart_trigger, 0);
        }
    }, 1);

    var glide_slope_tunnel_trigger = "/sim/remote/c172/glide-slope-tunnel";
    setlistener(glide_slope_tunnel_trigger, func(n) {
        if (n.getValue() == 1) {
            var p = "/sim/rendering/glide-slope-tunnel";
            setprop(p, var i = !getprop(p));
            gui.popupTip("Glide slope tunnel " ~ (i ? "enabled" : "disabled"));
            setprop(glide_slope_tunnel_trigger, 0);
        }
    }, 1);

    var set_property_delta_trigger = "/sim/remote/c172/set-property-delta";
    setlistener(set_property_delta_trigger, func(n) {
        var value = n.getValue();
        if (value != nil and value != "") {
            var parts = split(";", value);
            if (size(parts) >= 4) {
                var prop = parts[0];
                var delta = num(parts[1]);
                var display_name = parts[2];
                var format_string = parts[3];
                var prop2 = nil;
                var display_name2 = nil;
                if (size(parts) >= 6) {
                    prop2 = parts[4];
                    display_name2 = parts[5];
                }
                var cur = num(getprop(prop)) or 0;
                var updated = cur + delta;
                setprop(prop, updated);
                var msg = display_name ~ ": " ~ sprintf(format_string, updated);
                if (prop2 != nil and display_name2 != nil) {
                    var val2 = num(getprop(prop2)) or 0;
                    msg = msg ~ "; " ~ display_name2 ~ ": " ~ sprintf(format_string, val2);
                }
                gui.popupTip(msg);
            }
            setprop(set_property_delta_trigger, "");
        }
    }, 1);

    var set_property_value_trigger = "/sim/remote/c172/set-property-value";
    setlistener(set_property_value_trigger, func(n) {
        var value = n.getValue();
        if (value != nil and value != "") {
            var parts = split(";", value);
            if (size(parts) >= 4) {
                var prop = parts[0];
                var val = num(parts[1]);
                var display_name = parts[2];
                var format_string = parts[3];
                setprop(prop, val);
                gui.popupTip(display_name ~ ": " ~ sprintf(format_string, val));
            }
            setprop(set_property_value_trigger, "");
        }
    }, 1);

    var set_property_delta_modulo_trigger = "/sim/remote/c172/set-property-delta-modulo";
    setlistener(set_property_delta_modulo_trigger, func(n) {
        var value = n.getValue();
        if (value != nil and value != "") {
            var parts = split(";", value);
            if (size(parts) >= 5) {
                var prop = parts[0];
                var delta = num(parts[1]);
                var modulo = num(parts[2]);
                var display_name = parts[3];
                var format_string = parts[4];
                var prop2 = nil;
                var display_name2 = nil;
                if (size(parts) >= 7) {
                    prop2 = parts[5];
                    display_name2 = parts[6];
                }
                var cur = num(getprop(prop)) or 0;
                var updated = math.mod(cur + delta, modulo);
                if (modulo < 0) {
                    updated = -1 * updated;
                }
                setprop(prop, updated);
                var msg = display_name ~ ": " ~ sprintf(format_string, updated);
                if (prop2 != nil and display_name2 != nil) {
                    var val2 = num(getprop(prop2)) or 0;
                    msg = msg ~ "; " ~ display_name2 ~ ": " ~ sprintf(format_string, val2);
                }
                gui.popupTip(msg);
            }
            setprop(set_property_delta_modulo_trigger, "");
        }
    }, 1);

    var set_property_delta_range_trigger = "/sim/remote/c172/set-property-delta-range";
    setlistener(set_property_delta_range_trigger, func(n) {
        var value = n.getValue();
        if (value != nil and value != "") {
            var parts = split(";", value);
            if (size(parts) >= 6) {
                var prop = parts[0];
                var delta = num(parts[1]);
                var min_val = num(parts[2]);
                var max_val = num(parts[3]);
                var display_name = parts[4];
                var format_string = parts[5];
                var prop2 = nil;
                var display_name2 = nil;
                if (size(parts) >= 8) {
                    prop2 = parts[6];
                    display_name2 = parts[7];
                }
                var cur = num(getprop(prop)) or 0;
                var updated = cur + delta;
                if (updated < min_val) updated = min_val;
                if (updated > max_val) updated = max_val;
                setprop(prop, updated);
                var msg = display_name ~ ": " ~ sprintf(format_string, updated);
                if (prop2 != nil and display_name2 != nil) {
                    var val2 = num(getprop(prop2)) or 0;
                    msg = msg ~ "; " ~ display_name2 ~ ": " ~ sprintf(format_string, val2);
                }
                gui.popupTip(msg);
            }
            setprop(set_property_delta_range_trigger, "");
        }
    }, 1);

    var exchange_property_value_trigger = "/sim/remote/c172/exchange-property-values";
    setlistener(exchange_property_value_trigger, func(n) {
        var value = n.getValue();
        if (value != nil and value != "") {
            var parts = split(";", value);
            if (size(parts) >= 2) {
                var prop1 = parts[0];
                var prop2 = parts[1];
                var val1 = getprop(prop1);
                var val2 = getprop(prop2);
                setprop(prop1, val2);
                setprop(prop2, val1);
            }
            setprop(exchange_property_value_trigger, "");
        }
    }, 1);

    var toggle_boolean_property_trigger = "/sim/remote/c172/toggle-boolean-property";
    setlistener(toggle_boolean_property_trigger, func(n) {
        var value = n.getValue();
        if (value != nil and value != "") {
            var parts = split(";", value);
            if (size(parts) >= 2) {
                var prop = parts[0];
                var display_name = parts[1];
                var current = getprop(prop);
                var toggled = (current == "true") ? "false" : "true";
                setprop(prop, toggled);
                gui.popupTip(display_name ~ ": " ~ toggled);
            }
            setprop(toggle_boolean_property_trigger, "");
        }
    }, 1);

    var set_mag_starter_trigger = "/sim/remote/c172/set-mag-starter";
    setlistener(set_mag_starter_trigger, func(n) {
        var value = n.getValue();
        if (value != nil and value != "") {
            var val = num(value);
            # By default
            setprop("/controls/switches/starter", 0);
            setprop("/engines/active-engine/auto-start", 0);
            if (val >= 0 and val < 4) {
                setprop("/controls/switches/magnetos", val);
                gui.popupTip("magnetos: " ~ sprintf("%d", val))
            } else if (val == 4) {
                setprop("/controls/switches/starter", 1);
                setprop("/engines/active-engine/auto-start", 1);
                gui.popupTip("starter engaged");
            }
        }
        setprop(set_mag_starter_trigger, "");
    }, 1);

    var set_heading_indicator_delta_trigger = "/sim/remote/c172/set-heading-indicator-delta";
    setlistener(set_heading_indicator_delta_trigger, func(n) {
        var value = n.getValue();
        if (value != nil and value != "") {
            setprop("/instrumentation/heading-indicator/error-deg", 0);
            setprop("/instrumentation/heading-indicator/offset-deg", 0);
            var delta = num(value);
            var prop = "/instrumentation/heading-indicator/align-deg";
            var cur = num(getprop(prop)) or 0;
            var updated = math.mod(cur + delta, 360);
            setprop(prop, updated);
            var indicated_heading = num(getprop("/instrumentation/heading-indicator/indicated-heading-deg")) or 0;
            var indicated_compass = num(getprop("/instrumentation/magnetic-compass/indicated-heading-deg")) or 0;
            gui.popupTip("Heading: " ~ sprintf("%d", indicated_heading) ~ "; Compass: " ~ sprintf("%d", indicated_compass));
        }
        setprop(set_heading_indicator_delta_trigger, "");
    }, 1);

}
