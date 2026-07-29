import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { TauriProjectGateway } from "./lib/tauriProjectGateway";
import "./styles/tokens.css";
import "./styles/workbench.css";

const root = document.getElementById("root");
if (!root) throw new Error("Missing renderer root");

createRoot(root).render(
  <StrictMode><App gateway={new TauriProjectGateway()} /></StrictMode>,
);
