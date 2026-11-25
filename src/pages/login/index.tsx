import { type FormEvent, useState } from "react";
import { Link, Navigate, useLocation, useNavigate } from "react-router";

import { useAuth, useLoginMutation } from "@/api";
import { ApiError } from "@/api/client";
import { authErrorKey } from "@/lib/authErrors";
import { useT } from "@/lib/i18n";
import { AuthShell } from "@/ui/AuthShell";
import { Button } from "@/ui/shadcn/button";
import { Input } from "@/ui/shadcn/input";

type LoginLocationState = {
  from?: string;
};

export function LoginPage() {
  const t = useT();
  const { user } = useAuth();
  const login = useLoginMutation();
  const navigate = useNavigate();
  const location = useLocation();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");

  if (user) return <Navigate to="/" replace />;

  const from = (location.state as LoginLocationState | null)?.from ?? "/";
  const error = login.error;
  const errorKey = error instanceof ApiError ? authErrorKey(error.code) : null;
  const errorText = error ? (errorKey ? t(errorKey) : error.message) : null;

  function handleSubmit(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    login.mutate(
      { username, password },
      { onSuccess: () => void navigate(from, { replace: true }) },
    );
  }

  return (
    <AuthShell
      title={t("auth.loginTitle")}
      description={t("auth.loginDescription")}
      footer={
        <>
          {t("auth.loginHint")}{" "}
          <Link
            className="text-primary underline-offset-4 hover:underline"
            to="/register"
          >
            {t("auth.toRegister")}
          </Link>
        </>
      }
    >
      <form className="flex flex-col gap-3" onSubmit={handleSubmit}>
        <label className="flex flex-col gap-1.5">
          <span className="text-sm font-medium">{t("auth.username")}</span>
          <Input
            name="username"
            autoComplete="username"
            required
            minLength={3}
            maxLength={32}
            value={username}
            onChange={(event) => setUsername(event.target.value)}
          />
        </label>

        <label className="flex flex-col gap-1.5">
          <span className="text-sm font-medium">{t("auth.password")}</span>
          <Input
            name="password"
            type="password"
            autoComplete="current-password"
            required
            minLength={10}
            maxLength={128}
            value={password}
            onChange={(event) => setPassword(event.target.value)}
          />
        </label>

        {errorText && (
          <p role="alert" className="text-sm text-destructive">
            {errorText}
          </p>
        )}

        <Button type="submit" disabled={login.isPending}>
          {login.isPending ? t("auth.loggingIn") : t("auth.submitLogin")}
        </Button>
      </form>
    </AuthShell>
  );
}
