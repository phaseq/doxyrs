var my_link = undefined;

let sidebar = document.getElementById("sidebar");
for (const section of nav) {
    // <details>
    //     <summary>{{ section.title }}</summary>
    //     {% for link in section.children %}
    //     <a href="{{link.href | safe}}">{{link.text}}</a>
    //     {% endfor %}
    // </details>
    var details = document.createElement("details");

    var summary = document.createElement("summary");
    summary.textContent = section[0];
    details.appendChild(summary);

    for (const child of section[1]) {
        var link = document.createElement("a");
        link.textContent = child[0];
        link.href = child[1];

        if (link.href == document.location.href) {
            details.setAttribute('open', '');
            link.classList.add('current');
            my_link = link;
        }

        details.appendChild(link);
    }
    sidebar.appendChild(details);
}

if (my_link !== undefined) {
    my_link.scrollIntoView({
        behavior: 'auto',
        block: 'center',
        inline: 'center'
    });
}