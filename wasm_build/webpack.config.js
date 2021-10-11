// const CopyWebpackPlugin = require("copy-webpack-plugin");
const HtmlWebpackPlugin = require('html-webpack-plugin');
const path = require('path');
const fs = require('fs');

module.exports = {
    entry: path.join(__dirname, "index.js"),
    output: {
        path: path.resolve(__dirname, "dist"),
        filename: "bootstrap.js",
    },
    mode: "development",
    module: {
        rules: [
            {
                test: /\.?js$/,
                exclude: /node_modules/,
                use: {
                    loader: "babel-loader",
                    options: {
                        presets: ['@babel/preset-env']
                    }
                }
            },
        ]
    },
    plugins: [
        new HtmlWebpackPlugin({
            template: path.join(__dirname, "index.html"),
        }),
        // new CopyWebpackPlugin({
        //     patterns: [
        //         {
        //             from: 'index.html',
        //             to: 'dist/index.html',
        //         }
        //     ]
        // })
    ],
    devServer: {
        https: true,
        // key: fs.readFileSync("certs/server.key"),
        // cert: fs.readFileSync("certs/server.cert"),
        // cacert: fs.readFileSync("certs/cert.pem")
    }
};
