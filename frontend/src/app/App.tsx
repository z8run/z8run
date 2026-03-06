import { LoginPage } from "@/features/auth/LoginPage";
import { RegisterPage } from "@/features/auth/RegisterPage";
import { EditorPage } from "@/features/editor/EditorPage";
import { FlowListPage } from "@/features/flows/FlowListPage";
import { VaultPage } from "@/features/vault/VaultPage";
import { useAuthStore } from "@/stores/authStore";
import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";

function ProtectedRoute({ children }: { children: React.ReactNode }) {
  const token = useAuthStore((s) => s.token);
  if (!token) return <Navigate to="/login" replace />;
  return <>{children}</>;
}

export function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/login" element={<LoginPage />} />
        <Route path="/register" element={<RegisterPage />} />
        <Route
          path="/"
          element={
            <ProtectedRoute>
              <FlowListPage />
            </ProtectedRoute>
          }
        />
        <Route
          path="/flow/:id"
          element={
            <ProtectedRoute>
              <EditorPage />
            </ProtectedRoute>
          }
        />
        <Route
          path="/vault"
          element={
            <ProtectedRoute>
              <VaultPage />
            </ProtectedRoute>
          }
        />
      </Routes>
    </BrowserRouter>
  );
}
