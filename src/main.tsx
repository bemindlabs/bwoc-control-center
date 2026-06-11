import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";

// Calm is light-first with a dark variant; a fleet console defaults to dark,
// but follow the OS when it asks for light.
if (!window.matchMedia("(prefers-color-scheme: light)").matches) {
  document.documentElement.classList.add("dark");
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
