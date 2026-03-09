import { AuthFormLayout } from "@/components/ui/AuthFormLayout";
import { formInputClass, formLabelClass } from "@/lib/styles";
import { useAuthStore } from "@/stores/authStore";
import { LogIn } from "lucide-react";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";

export function LoginPage() {
  const { login, loading, error, token, clearError } = useAuthStore();
  const navigate = useNavigate();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");

  useEffect(() => {
    if (token) navigate("/");
  }, [token, navigate]);

  useEffect(() => {
    clearError();
  }, [clearError]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    await login(email, password);
  };

  return (
    <AuthFormLayout
      title="Sign in"
      error={error}
      submitLabel="Sign in"
      submitLoadingLabel="Signing in..."
      submitIcon={LogIn}
      loading={loading}
      onSubmit={handleSubmit}
      footerText="Don't have an account?"
      footerLinkText="Create one"
      footerLinkTo="/register"
    >
      <div>
        <label htmlFor="login-email" className={formLabelClass}>
          Email
        </label>
        <input
          id="login-email"
          type="email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          className={formInputClass}
          placeholder="you@example.com"
          required
        />
      </div>
      <div>
        <label htmlFor="login-password" className={formLabelClass}>
          Password
        </label>
        <input
          id="login-password"
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          className={formInputClass}
          placeholder="••••••••"
          required
        />
      </div>
    </AuthFormLayout>
  );
}
