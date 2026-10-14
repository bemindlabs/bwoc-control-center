import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";

// Calm is light-first with a dark variant; a fleet console defaults to dark,
// but follow the OS when it asks for light — including live theme switches.
const prefersLight = window.matchMedia("(prefers-color-scheme: light)");
const applyTheme = () =>
  document.documentElement.classList.toggle("dark", !prefersLight.matches);
applyTheme();
prefersLight.addEventListener("change", applyTheme);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
