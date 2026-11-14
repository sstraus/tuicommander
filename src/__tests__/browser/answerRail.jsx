import { render } from "solid-js/web";
import { Transcript } from "../../components/AIChatPanel/Transcript";
import "../../global.css";

// The supplied screenshot's text, transcribed with local OCR. Marker positions
// exercise the reported mixed-list shape; no session or backend is attached.
const reported = [
  "💬 Sulle due storie del batch UI c’era già del lavoro, quindi `tuic-ui-batch` non serve più.\n\n- La **2843-734d** ha già una correzione committata (`daaa7b1db6`) su `fix/2843-mobile-slash-send`, con il suo worktree e senza proprietario: era lavoro rimasto in sospeso. La sto facendo atterrare direttamente (rischio basso, nessun critic: è un cambio UI).\n- 💬 La **2462-5839** ha un branch con il solo audit diagnostico, e nessuna correzione. Resta aperta e senza proprietario.\n\n💬 Sul perché il pulsante Invio mobile manchi i tap non abbiamo ancora una riproduzione. Lo mettiamo su un agente solo se lo vuoi.",
  "Da confermare: ack 254.",
  "💬 Boss, il fix della **2843** è landato, ma la storia non si chiude ancora.\n\n- La **2843-734d** (invio mobile di `/help` che lasciava il composer sporco) è su `main` (`9fea8f4198`), ma `story-close` rifiuta: il criterio 1 (invio mobile di `/help` a una sessione Claude, comando eseguito una volta e composer svuotato) non è provato da una verifica reale. Il commit c’era già su un branch rimasto in sospeso e nessuno aveva portato la prova.",
];
const examples = "- Context before.\n- 💬 Answer with `inline code`.\n  - Nested context.\n- Context after.\n\n3. 💬 Ordered answer.\n4. 💬 Next ordered answer.\n\n- Unmarked parent\n  - 💬 Nested answer.\n  - Unmarked sibling.";
const entries = [...reported, examples].map((text, i) => ({ id: String(i), kind: "agent", text }));
render(() => <main style={{ height: "100vh", width: "100%" }}><Transcript entries={() => entries} busy={() => false} emptyMessage="" terminalAgent="ego" observeToolDuration={false} /></main>, document.getElementById("root"));
window.answerRailGeometry = () => {
  const root = document.querySelector('[aria-label="Chat transcript"]');
  root.scrollTop = 0;
  return [...document.querySelectorAll('#markdown-content *')].filter(el => getComputedStyle(el).borderLeftColor === getComputedStyle(document.documentElement).getPropertyValue('--answer-rule').trim() && parseFloat(getComputedStyle(el).borderLeftWidth) > 0).map(el => ({ tag: el.tagName, x: el.getBoundingClientRect().x, y: el.getBoundingClientRect().y, height: el.getBoundingClientRect().height, parent: el.parentElement.tagName, text: el.textContent.slice(0, 60) }));
};
