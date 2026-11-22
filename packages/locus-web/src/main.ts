import source from "virtual:perish/views";
import { Views } from "@perishlab/design";
import { mount } from "svelte";
import "./style.css";

const target = document.getElementById("app");
if (target) mount(Views, { target, props: { source } });
