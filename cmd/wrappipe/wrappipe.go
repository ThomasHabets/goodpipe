/*
	Copyright 2022 Google LLC

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
/*
   * WARNING: Status of this is "ugly hack"
   *
 * Example usage: wrappipe -o echo 'here starts <_hello world]> end' | wrappipe -i cat
*/
package main

import (
	"context"
	"errors"
	"flag"
	"io"
	"os"
	"os/exec"
	"strings"
	"sync"

	log "github.com/sirupsen/logrus"
)

var (
	input  = flag.Bool("i", false, "")
	output = flag.Bool("o", false, "")
)

const (
	eof = byte('Z')

	esc = byte('_')
	sob = byte('<')
	eob = byte('>')

	eesc = byte('-')
	esob = byte('[')
	eeob = byte(']')
)

type encapper struct{}

func (encapper) EOF() {
	if _, err := os.Stdout.Write([]byte{eof}); err != nil {
		log.Fatal(err)
	}
}

func (encapper) Write(in []byte) (int, error) {
	s := string(in)

	// Escape escape.
	s = strings.ReplaceAll(s, string([]byte{esc}), string([]byte{esc, eesc}))

	// Escape sob/eob.
	s = strings.ReplaceAll(s, string([]byte{sob}), string([]byte{esc, esob}))
	s = strings.ReplaceAll(s, string([]byte{eob}), string([]byte{esc, eeob}))

	out := []byte(string([]byte{sob}) + s + string([]byte{eob}))
	n, err := os.Stdout.Write(out)
	if n != len(out) {
		log.Fatal("Short write for some reason") // TODO
	}
	return len(in), err
}

const (
	stateIdle = iota
	stateData
	stateDataEsc
)

type decapper struct {
	state     int
	succeeded bool
	out       io.Writer
}

func (d *decapper) Write(in []byte) (int, error) {
	if d.state == stateIdle {
		switch in[0] {
		case sob:
			d.state = stateData
			n, err := d.Write(in[1:])
			return n + 1, err
		case eof:
			d.succeeded = true
			return 1, nil
		}
	}

	var o []byte
	for n := 0; n < len(in); n++ {
		switch d.state {
		case stateData:
			switch in[n] {
			case esc:
				d.state = stateDataEsc
			case eob:
				d.state = stateIdle
			default:
				o = append(o, in[n])
			}
		case stateDataEsc:
			switch in[n] {
			case esob:
				d.state = stateData
				o = append(o, sob)
			case eeob:
				d.state = stateData
				o = append(o, eob)
			case eesc:
				d.state = stateData
				o = append(o, esc)
			default:
				log.Fatal("invalid esc")
			}
		}
	}
	if _, err := d.out.Write(o); err != nil {
		return 0, err
	}
	return len(in), nil
}

func main() {
	flag.Parse()

	ctx, cancel := context.WithCancel(context.Background())

	cmd := exec.CommandContext(ctx, flag.Arg(0), flag.Args()[1:]...)
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr

	var wg sync.WaitGroup
	indone := make(chan bool)
	inclose := func() {}
	if *input {
		r, w, err := os.Pipe()
		if err != nil {
			log.Fatal(err)
		}
		cmd.Stdin = r
		inclose = func() {
			w.Close()
			os.Stdin.Close()
		}
		wg.Add(1)
		go func() {
			defer wg.Done()
			d := decapper{
				out: w,
			}
			if _, err := io.Copy(&d, os.Stdin); errors.Unwrap(err) == os.ErrClosed {
				// TODO: is this an error?
				log.Errorf("Program %q exited before consuming all input", flag.Arg(0))
				return
			} else if err != nil {
				log.Fatalf("Copying from stdin to program: %v", err)
			}
			if !d.succeeded {
				log.Errorf("Upstream data for %q finished before sending EOF", flag.Arg(0))
				cancel()
				<-indone
			}
			w.Close()
		}()
	}

	outclose := func() {}
	if *output {
		r, w, err := os.Pipe()
		if err != nil {
			log.Fatal(err)
		}
		outclose = func() {
			w.Close()
		}
		cmd.Stdout = w
		wg.Add(1)
		go func() {
			defer wg.Done()
			var e encapper
			if _, err := io.Copy(e, r); err != nil {
				log.Fatalf("Copying from program to stdout: %v", err)
			}
			e.EOF()
		}()
	}
	if err := cmd.Run(); err != nil {
		log.Fatalf("Command %q failed: %v", flag.Arg(0), err)
	}
	close(indone)
	outclose()
	inclose()
	wg.Wait()
}
