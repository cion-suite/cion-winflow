import { useMutation } from "@tanstack/react-query";
import { call } from "./client";

export const greeting = {
  useGreet: () =>
    useMutation({
      mutationFn: (name: string) => call<string>("greet", { name }),
    }),
};
