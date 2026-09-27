import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { Pet } from "./pet/Pet";
import { Takeover } from "./takeover/Takeover";
import "./styles.css";

// One bundle, three windows: inbox (default), desktop pet (?view=pet),
// critical takeover (?view=takeover).
const view = new URLSearchParams(window.location.search).get("view");
if (view === "pet" || view === "takeover") document.documentElement.classList.add(`${view}-mode`);

const root = document.getElementById("root");
if (!root) throw new Error("#root missing from index.html");
createRoot(root).render(
  <StrictMode>{view === "pet" ? <Pet /> : view === "takeover" ? <Takeover /> : <App />}</StrictMode>,
);
