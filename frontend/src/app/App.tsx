import { BrowserRouter, Routes, Route } from "react-router-dom";
import { FlowListPage } from "@/features/flows/FlowListPage";
import { EditorPage } from "@/features/editor/EditorPage";

export function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<FlowListPage />} />
        <Route path="/flow/:id" element={<EditorPage />} />
      </Routes>
    </BrowserRouter>
  );
}
