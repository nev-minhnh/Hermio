import "./app.css";
import { mount } from "svelte";
import App from "./App.svelte";

// Pick the theme before first paint to avoid a flash.
document.documentElement.dataset.theme = matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";

const app = mount(App, { target: document.getElementById("app")! });
export default app;
