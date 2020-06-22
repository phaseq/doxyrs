var my_link = undefined;

// {% for section in sections %}
// <details>
//     <summary>{{ section.title }}</summary>
//     {% for link in section.children %}
//     <a href="{{link.href | safe}}">{{link.text}}</a>
//     {% endfor %}
//     {% for section in section.sections %}
//     [recurse]
//     {% endfor %}
// </details>
// {% endfor %}
function addSections(container, content) {
    var containsLink = false;

    let page = content[0];
    let subpages = content[1];

    // create link to this page
    var link = document.createElement("a");
    link.textContent = page[0];
    link.href = pathToRoot + page[1];
    if (link.href == document.location.href.split('#')[0]) {
        link.classList.add('current');
        my_link = link;
        containsLink = true;
    }

    if (subpages.length == 0) {
        container.appendChild(link);
    }
    else {
        // create summary element
        var summary = document.createElement("summary");
        if (link.href == document.location.href) {
            summary.classList.add('current');
        }
        summary.appendChild(link);

        // create details element
        var details = document.createElement("details");
        if (containsLink) {
            details.setAttribute('open', '');
        }
        details.appendChild(summary);

        // create subpage elements
        for (let subpage of subpages) {
            let childContainsLink = addSections(details, subpage);
            if (childContainsLink) {
                details.setAttribute('open', '');
                containsLink = true;
            }
        }
        container.appendChild(details);
    }
    return containsLink;
}

let sidebar = document.getElementById("sidebar");
addSections(sidebar, nav[0]);

if (my_link !== undefined) {
    my_link.scrollIntoView({
        behavior: 'auto',
        block: 'center',
        inline: 'center'
    });
}