import type { ReactNode } from "react";

import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/ui/shadcn/card";

type AuthShellProps = {
  title: string;
  description: string;
  children: ReactNode;
  footer?: ReactNode;
};

/** Общая рамка экранов входа и регистрации: карточка по центру окна. */
export function AuthShell({ title, description, children, footer }: AuthShellProps) {
  return (
    <main className="grid min-h-svh place-items-center bg-muted/30 p-6">
      <Card className="w-full max-w-sm">
        <CardHeader>
          <CardTitle className="text-xl">{title}</CardTitle>
          <CardDescription>{description}</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          {children}
          {footer && <div className="text-sm text-muted-foreground">{footer}</div>}
        </CardContent>
      </Card>
    </main>
  );
}
