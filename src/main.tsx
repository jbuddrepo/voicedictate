import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import Overlay from "./Overlay";
import "./styles.css";

if (window.location.hash === "#overlay") document.documentElement.classList.add("overlay-document");

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {window.location.hash === "#overlay" ? <Overlay /> : <App />}
  </React.StrictMode>
);
