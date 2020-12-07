let json = JSON.parse(document.getElementById("groups-json").innerHTML);
for (let group of json) {
    for (let element of group.elements) {
        element.visible = true;
        element.node_visible = true;
    }
    group.visible = true;
    group.num_elements_visible = group.elements.length;
}

let input = document.getElementById("ip");

let update_shows = () => {
    let new_input = "";
    for (let char of input.value) {
        char = char.codePointAt(0);
        if (char >= 65 && char <= 90) {
            char += 32;
        }
        if (char > 127 || (char >= 48 && char <= 57) || (char >= 97 && char <= 122)) {
            new_input += String.fromCodePoint(char);
        }
    }
    let display_changes = [];
    for (let group of json) {
        let group_changed = false;
        for (let element of group.elements) {
            let old_visible = element.visible;
            element.visible = element.names.some((n) => n.includes(new_input));
            if (element.visible !== old_visible) {
                group_changed = true;
                group.num_elements_visible += element.visible ? 1 : -1;
            }
        }
        if (group_changed) {
            let old_visible = group.visible;
            group.visible = group.always_visible || group.num_elements_visible > 0;
            if (group.visible !== old_visible) {
                group.node = group.node || document.getElementById("group-" + group.name);
                group.lb_node = group.lb_node || document.getElementById("lb-" + group.name);
                display_changes.push([group.node, group.visible ? "" : "none"]);
                display_changes.push([group.lb_node, group.visible ? "" : "none"]);
            }
            if (group.visible) {
                for (let element of group.elements) {
                    if (element.node_visible !== element.visible) {
                        element.node_visible = element.visible;
                        element.node = element.node || document.getElementById("element-" + element.element_id);
                        display_changes.push([element.node, element.visible ? "" : "none"]);
                    }
                }
            }
        }
    }
    if (display_changes.length > 0) {
        for (let [node, prop] of display_changes) {
            node.style.display = prop;
        }
    }
};
input.addEventListener("input", update_shows);

window.onload = update_shows;
