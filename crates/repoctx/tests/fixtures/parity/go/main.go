package main

type Server struct {
	Port int
}

func (s Server) Start() {}

func NewServer() Server { return Server{} }

const DefaultPort = 8080
