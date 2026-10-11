import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource-variable/inter";
import "@fontsource/cormorant-garamond/500.css";
import "@fontsource/cormorant-garamond/500-italic.css";
import "@fontsource/cormorant-garamond/600.css";
import { App } from "./App";
import "./styles/tokens.css";
import "./styles/app.css";
import "./styles/generation.css";
import "./styles/view.css";
import "./styles/editor.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
