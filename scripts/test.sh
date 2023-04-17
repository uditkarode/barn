#!/bin/bash
echo "      :not error"
echo "also not  "
sleep 1
>&2 echo "error"
echo "yes"
