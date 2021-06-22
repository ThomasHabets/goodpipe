package main

import (
	"strings"
	"testing"
)

func TestThings(t *testing.T) {
	tests := []struct {
		name  string
		input string
		exit  int
	}{
		/*
			{
				"Empty",
				`[]`,
				0,
			},
		*/
		{
			"Trivial",
			`[["true"]]`,
			0,
		},
		{
			"Trivial fail",
			`[["false"]]`,
			1,
		},
		{
			"Bad command",
			`[["/non/existing/binary"]]`,
			-1,
		},
		{
			"Two commands",
			`[["cat", "/dev/null"], ["cat"]]`,
			0,
		},
		{
			"First fail",
			`[["cat", "/non/existing"], ["cat"]]`,
			1,
		},
		{
			"Second fail",
			`[["cat", "/dev/null"], ["dd", "of=/non/existing/file"]]`,
			1,
		},
		{
			"Last fail",
			`[["cat", "/dev/null"], ["cat"], ["dd", "of=/non/existing/file"]]`,
			1,
		},
		{
			"Mid fail",
			`[["cat", "/dev/null"], ["blah"], ["cat"]]`,
			-1,
		},
	}
	for _, test := range tests {
		if got, want := run(strings.NewReader(test.input)), test.exit; got != want {
			t.Errorf("%s: Wrong error code returned. got %d, want %d", test.name, got, want)
		}
	}
}
