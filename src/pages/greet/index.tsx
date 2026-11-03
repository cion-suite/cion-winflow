import { useState } from "react";
import type { SubmitEvent } from "react";
import { api } from "@/api";
import { Button } from "@/ui/shadcn/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/ui/shadcn/card";
import { Input } from "@/ui/shadcn/input";

export function GreetPage() {
  const [name, setName] = useState("");
  const greet = api.greeting.useGreet();

  function handleSubmit(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    greet.mutate(name);
  }

  return (
    <div className="flex flex-1 items-center justify-center">
      <Card className="w-full max-w-sm">
        <CardHeader>
          <CardTitle>
            <h1>cc-tweaks</h1>
          </CardTitle>
          <CardDescription>
            React calls a Rust command through Tauri.
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <form className="flex gap-2" onSubmit={handleSubmit}>
            <Input
              aria-label="Name"
              placeholder="Enter a name..."
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
            <Button type="submit">Greet</Button>
          </form>
          <p role="status" className="text-sm">
            {greet.data}
          </p>
          {greet.error && (
            <p role="alert" className="text-sm text-destructive">
              {greet.error.message}
            </p>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
