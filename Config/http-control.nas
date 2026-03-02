var _http_control_code = func() {

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
                var cur = num(getprop(prop)) or 0;
                var updated = cur + delta;
                setprop(prop, updated);
                gui.popupTip(display_name ~ ": " ~ sprintf(format_string, updated));
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
                var cur = num(getprop(prop)) or 0;
                var updated = math.mod(cur + delta, modulo);
                setprop(prop, updated);
                gui.popupTip(display_name ~ ": " ~ sprintf(format_string, updated));
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
                var cur = num(getprop(prop)) or 0;
                var updated = cur + delta;
                if (updated < min_val) updated = min_val;
                if (updated > max_val) updated = max_val;
                setprop(prop, updated);
                gui.popupTip(display_name ~ ": " ~ sprintf(format_string, updated));
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
}

setlistener("/sim/signals/nasal-dir-initialized", _http_control_code);
