import { type FormEvent, useState } from "react";
import { Link, Navigate, useNavigate } from "react-router";

import { useAuth, useRegisterMutation } from "@/api";
import { ApiError } from "@/api/client";
import { authErrorKey } from "@/lib/authErrors";
import { useT } from "@/lib/i18n";
import { AuthShell } from "@/ui/AuthShell";
import { Button } from "@/ui/shadcn/button";
import { Input } from "@/ui/shadcn/input";

export function RegisterPage() {
  const t = useT();
  const { user } = useAuth();
  const register = useRegisterMutation();
  const navigate = useNavigate();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [invite, setInvite] = useState("");

  if (user) return <Navigate to="/" replace />;

  const error = register.error;
  const errorKey = error instanceof ApiError ? authErrorKey(error.code) : null;
  const errorText = error ? (errorKey ? t(errorKey) : error.message) : null;

  function handleSubmit(event: FormEvent<HTMLFormElement>): void {
    event.preventDefault();
    register.mutate(
      { username, password, invite },
      { onSuccess: () => void navigate("/", { replace: true }) },
    );
  }

  return (
    <AuthShell
      title={t("auth.registerTitle")}
      description={t("auth.registerDescription")}
      footer={
        <>
          {t("auth.registerHint")}{" "}
          <Link
            className="text-primary underline-offset-4 hover:underline"
            to="/login"
          >
            {t("auth.toLogin")}
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
            autoComplete="new-password"
            required
            minLength={10}
            maxLength={128}
            value={password}
            onChange={(event) => setPassword(event.target.value)}
          />
        </label>

        <label className="flex flex-col gap-1.5">
          <span className="text-sm font-medium">{t("auth.invite")}</span>
          <Input
            name="invite"
            autoComplete="off"
            required
            value={invite}
            onChange={(event) => setInvite(event.target.value)}
          />
        </label>

        {errorText && (
          <p role="alert" className="text-sm text-destructive">
            {errorText}
          </p>
        )}

        <Button type="submit" disabled={register.isPending}>
          {register.isPending ? t("auth.registering") : t("auth.submitRegister")}
        </Button>
      </form>
    </AuthShell>
  );
}
