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
    for (const page of content.pages) {
        var link = document.createElement("a");
        link.textContent = page[0];
        link.href = page[1];

        if (link.href == document.location.href) {
            link.classList.add('current');
            my_link = link;
            containsLink = true;
        }

        container.appendChild(link);
    }
    for (let [section_title, section] of content.sections) {
        var details = document.createElement("details");

        var summary = document.createElement("summary");
        summary.textContent = section_title.replace(/_/g, ' '); // TODO: remove replace
        details.appendChild(summary);

        if (section.hasOwnProperty("sections")) {
            let childContainsLink = addSections(details, section);
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
addSections(sidebar, nav);

if (my_link !== undefined) {
    my_link.scrollIntoView({
        behavior: 'auto',
        block: 'center',
        inline: 'center'
    });
}