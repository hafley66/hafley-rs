:- module(rehome, []).
:- use_module(library(lists)).
:- include('part.pl').
:- reexport('other.pl').
:- reexport('third.pl', [foo/1, bar//0]).
hello.
