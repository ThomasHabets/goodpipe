/*
   Copyright 2021 Google LLC

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    https://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
*/
package main

import (
	"context"
	"encoding/json"
	"flag"
	"io"
	"io/ioutil"
	"os"
	"os/exec"

	log "github.com/sirupsen/logrus"
)

type exit struct {
	valid bool
	ret   int
}

func createPipe(ctx context.Context, args [][]string, in io.Reader, done chan<- exit) {
	defer close(done)

	cmd := exec.CommandContext(ctx, args[0][0], args[0][1:]...)
	cmd.Stdin = in
	cmd.Stderr = os.Stderr
	cmd.Stdout = os.Stdout

	ctx2, cancel := context.WithCancel(ctx)
	done2 := make(chan exit, 1)

	var w io.WriteCloser
	if len(args) > 1 {
		var err error
		var r io.Reader
		r, w, err = os.Pipe()
		if err != nil {
			log.Fatalf("Creating pipe: %v", err)
		}
		defer w.Close()
		cmd.Stdout = w
		go createPipe(ctx2, args[1:], r, done2)
	} else {
		close(done2)
	}
	if err := cmd.Run(); err != nil {
		log.Errorf("Failed to run %q: %v", args[0][0], err)
		cancel()
	}
	if w != nil {
		w.Close()
	}
	next := <-done2
	if code := cmd.ProcessState.ExitCode(); code == 0 && w != nil {
		// If this part succeeded, pass along the exit code
		// from the next step.
		done <- next
	} else {
		done <- exit{
			valid: true,
			ret:   code,
		}
	}
}

func main() {
	flag.Parse()
	ctx := context.Background()
	if flag.NArg() != 0 {
		log.Fatalf("Trailing args on cmdline: %q", flag.Args())
	}

	b, err := ioutil.ReadAll(os.Stdin)
	if err != nil {
		log.Fatalf("Failed to read config from stdin: %v", err)
	}
	var pipes [][]string
	if err := json.Unmarshal(b, &pipes); err != nil {
		log.Fatalf("Failed to parse json: %v", err)
	}
	log.Infof("Running…")
	done := make(chan exit, 1)
	createPipe(ctx, pipes, os.Stdin, done)
	rc := <-done
	if !rc.valid {
		log.Fatal("Did not get a valid return code for pipeline.")
	}
	os.Exit(rc.ret)
}
