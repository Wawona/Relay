// WASI P1 demo (Go 1.21+: GOOS=wasip1 GOARCH=wasm).
package main

import (
	"fmt"
	"os"
)

func main() {
	args := os.Args
	// Default to hello so `wasm hello-wasi` is a useful smoke (stdout).
	// Extra subcommands stay available for fs-* / escape tests.
	cmd := "hello"
	if len(args) > 1 {
		cmd = args[1]
	}
	switch cmd {
	case "hello":
		fmt.Println("hello from wawona wasm-demo-go")
		fmt.Printf("argv = %v\n", args)
		fmt.Printf("HOME = %s\n", os.Getenv("HOME"))
	case "fs-write":
		path := "demo.txt"
		if len(args) > 2 {
			path = args[2]
		}
		if err := os.WriteFile(path, []byte("wawona wasm fs-write\n"), 0644); err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}
		fmt.Println("wrote", path)
	case "fs-read":
		path := "demo.txt"
		if len(args) > 2 {
			path = args[2]
		}
		b, err := os.ReadFile(path)
		if err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}
		fmt.Print(string(b))
	case "fs-escape":
		if _, err := os.ReadFile("../../outside.txt"); err == nil {
			fmt.Fprintln(os.Stderr, "FAIL: escape succeeded")
			os.Exit(2)
		} else {
			fmt.Println("fs-escape denied as expected:", err)
		}
	default:
		fmt.Fprintln(os.Stderr, "usage: wasm-demo-go hello|fs-write|fs-read|fs-escape")
		os.Exit(1)
	}
}
