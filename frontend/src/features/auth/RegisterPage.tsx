import { useAuthStore } from "@/stores/authStore";
import { Loader2, UserPlus } from "lucide-react";
import { useEffect, useState } from "react";
import { Link, useNavigate } from "react-router-dom";

export function RegisterPage() {
  const { register, loading, error, token, clearError } = useAuthStore();
  const navigate = useNavigate();
  const [email, setEmail] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");

  useEffect(() => {
    if (token) navigate("/");
  }, [token, navigate]);

  useEffect(() => {
    clearError();
  }, [clearError]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    await register(email, username, password);
  };

  return (
    <div className="min-h-screen bg-slate-950 flex items-center justify-center">
      <div className="w-full max-w-sm mx-4">
        {/* Logo */}
        <div className="flex items-center justify-center gap-3 mb-8">
          <div className="w-10 h-10 rounded-lg bg-z8-600 flex items-center justify-center">
            <span className="text-sm font-bold text-white">z8</span>
          </div>
          <div>
            <h1 className="text-xl font-semibold text-slate-100">z8run</h1>
            <p className="text-xs text-slate-500">Flow Engine</p>
          </div>
        </div>

        {/* Form card */}
        <div className="bg-slate-900 border border-slate-800 rounded-lg p-6">
          <h2 className="text-lg font-medium text-slate-200 mb-6">
            Create account
          </h2>

          {error && (
            <div className="mb-4 p-3 bg-red-900/30 border border-red-800 rounded-md text-sm text-red-300">
              {error}
            </div>
          )}

          <form onSubmit={handleSubmit} className="space-y-4">
            <div>
              <label
                htmlFor="register-email"
                className="block text-xs text-slate-400 mb-1.5"
              >
                Email
              </label>
              <input
                id="register-email"
                type="email"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                className="w-full bg-slate-800 border border-slate-700 rounded-md px-3 py-2
                  text-sm text-slate-200 placeholder-slate-500 focus:outline-none focus:border-z8-500"
                placeholder="you@example.com"
                required
              />
            </div>
            <div>
              <label
                htmlFor="register-username"
                className="block text-xs text-slate-400 mb-1.5"
              >
                Username
              </label>
              <input
                id="register-username"
                type="text"
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                className="w-full bg-slate-800 border border-slate-700 rounded-md px-3 py-2
                  text-sm text-slate-200 placeholder-slate-500 focus:outline-none focus:border-z8-500"
                placeholder="username"
                required
              />
            </div>
            <div>
              <label
                htmlFor="register-password"
                className="block text-xs text-slate-400 mb-1.5"
              >
                Password
              </label>
              <input
                id="register-password"
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                className="w-full bg-slate-800 border border-slate-700 rounded-md px-3 py-2
                  text-sm text-slate-200 placeholder-slate-500 focus:outline-none focus:border-z8-500"
                placeholder="••••••••"
                required
              />
            </div>
            <button
              type="submit"
              disabled={loading}
              className="w-full flex items-center justify-center gap-2 px-4 py-2.5 bg-z8-600
                hover:bg-z8-700 text-white text-sm font-medium rounded-lg transition-colors
                disabled:opacity-60"
            >
              {loading ? (
                <Loader2 size={16} className="animate-spin" />
              ) : (
                <UserPlus size={16} />
              )}
              {loading ? "Creating account..." : "Create account"}
            </button>
          </form>

          <p className="mt-4 text-center text-xs text-slate-500">
            Already have an account?{" "}
            <Link to="/login" className="text-z8-400 hover:text-z8-300">
              Sign in
            </Link>
          </p>
        </div>
      </div>
    </div>
  );
}
