import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { Pet } from "./pet/Pet";
import "./styles.css";

// One bundle, two windows: the inbox (default) and the desktop pet (?view=pet).
const isPet = new URLSearchParams(window.location.search).get("view") === "pet";
if (isPet) document.documentElement.classList.add("pet-mode");

const root = document.getElementById("root");
if (!root) throw new Error("#root missing from index.html");
createRoot(root).render(<StrictMode>{isPet ? <Pet /> : <App />}</StrictMode>);
