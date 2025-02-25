function toggleTheme() {
    const html = document.documentElement;
    const currentTheme = html.getAttribute("data-bs-theme");
    const newTheme = currentTheme === "dark" ? "light" : "dark";
    html.setAttribute("data-bs-theme", newTheme);
    localStorage.setItem("theme", newTheme);
    applyTheme(newTheme);
}

window.addEventListener("DOMContentLoaded", () => {
    const storedTheme = localStorage.getItem("theme") || "dark";
    document.documentElement.setAttribute("data-bs-theme", storedTheme);
    document.getElementById("theme-icon-sun").style.display = storedTheme === "dark" ? "inline-block" : "none";
    document.getElementById("theme-icon-moon").style.display = storedTheme === "dark" ? "none" : "inline-block";
    applyTheme(storedTheme);
});

function applyTheme(theme) {
    document.getElementById("theme-icon-sun").style.display = theme === "dark" ? "inline-block" : "none";
    document.getElementById("theme-icon-moon").style.display = theme === "dark" ? "none" : "inline-block";
    document.getElementById("cyclonetix-light").disabled = theme === "dark";
    document.getElementById("cyclonetix-dark").disabled = theme !== "dark";
    document.getElementById("tabulator-light").disabled = theme === "dark";
    document.getElementById("tabulator-dark").disabled = theme !== "dark";
}
