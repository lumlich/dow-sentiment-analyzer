import './setupFetch';
import { render } from "preact";
import { App } from "./app";

const root = document.getElementById("root")!;
console.log("Mounting App into", root);
render(<App />, root);