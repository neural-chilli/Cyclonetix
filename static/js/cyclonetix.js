// Toggle theme when the user clicks a button or icon.
function toggleTheme() {
    const html = document.documentElement;
    // We're using data-bs-theme here; adjust if your project uses data-theme.
    const current = html.getAttribute("data-bs-theme");
    const newTheme = current === "dark" ? "light" : "dark";
    html.setAttribute("data-bs-theme", newTheme);
    localStorage.setItem("theme", newTheme);

    // Show/hide the appropriate icons.
    const sunIcon = document.getElementById("theme-icon-sun");
    const moonIcon = document.getElementById("theme-icon-moon");
    if (newTheme === "dark") {
        sunIcon.style.display = "inline-block";
        moonIcon.style.display = "none";
    } else {
        sunIcon.style.display = "none";
        moonIcon.style.display = "inline-block";
    }
}

// On page load, set the theme from localStorage.
window.addEventListener("DOMContentLoaded", () => {
    const storedTheme = localStorage.getItem("theme") || "dark";
    const html = document.documentElement;
    html.setAttribute("data-bs-theme", storedTheme);

    // Set the icon visibility.
    const sunIcon = document.getElementById("theme-icon-sun");
    const moonIcon = document.getElementById("theme-icon-moon");
    if (storedTheme === "dark") {
        sunIcon.style.display = "inline-block";
        moonIcon.style.display = "none";
    } else {
        sunIcon.style.display = "none";
        moonIcon.style.display = "inline-block";
    }
});