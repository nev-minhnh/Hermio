import "./app.css";
import { mount } from "svelte";
import App from "./App.svelte";
import NotifyPopup from "./components/NotifyPopup.svelte";

// Pick the theme before first paint to avoid a flash.
document.documentElement.dataset.theme = matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";

// The new-mail popup window loads the same page with #notify.
const app = mount(location.hash === "#notify" ? NotifyPopup : App, { target: document.getElementById("app")! });
export default app;
