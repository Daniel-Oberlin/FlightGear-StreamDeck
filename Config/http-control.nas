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
}

setlistener("/sim/signals/nasal-dir-initialized", _http_control_code);
