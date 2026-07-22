import { AuthFormLayout } from "@/components/ui/AuthFormLayout";
import { formInputClass, formLabelClass } from "@/lib/styles";
import { useAuthStore } from "@/stores/authStore";
import { UserPlus } from "lucide-react";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";

export function RegisterPage() {
  const { register, loading, error, user, clearError } = useAuthStore();
  const navigate = useNavigate();
  const [email, setEmail] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");

  useEffect(() => {
    if (user) navigate("/");
  }, [user, navigate]);

  useEffect(() => {
    clearError();
  }, [clearError]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    await register(email, username, password);
  };

  return (
    <AuthFormLayout
      title="Create account"
      error={error}
      submitLabel="Create account"
      submitLoadingLabel="Creating account..."
      submitIcon={UserPlus}
      loading={loading}
      onSubmit={handleSubmit}
      footerText="Already have an account?"
      footerLinkText="Sign in"
      footerLinkTo="/login"
    >
      <div>
        <label htmlFor="register-email" className={formLabelClass}>
          Email
        </label>
        <input
          id="register-email"
          type="email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          className={formInputClass}
          placeholder="you@example.com"
          required
        />
      </div>
      <div>
        <label htmlFor="register-username" className={formLabelClass}>
          Username
        </label>
        <input
          id="register-username"
          type="text"
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          className={formInputClass}
          placeholder="username"
          required
        />
      </div>
      <div>
        <label htmlFor="register-password" className={formLabelClass}>
          Password
        </label>
        <input
          id="register-password"
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
